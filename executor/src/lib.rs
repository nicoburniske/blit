#![feature(must_not_suspend)]
#![deny(must_not_suspend)]
//! local task execution for Blit platform runners

use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{AsyncFnOnce, Deref, DerefMut},
    panic::Location,
    ptr::NonNull,
    rc::Rc,
};

mod scope;
mod task;

pub use scope::{Scope, ScopeRef};
pub use task::TaskId;

/// maps application state to nested state when it is available
pub trait Project<T: 'static> {
    fn project(&mut self) -> Option<&mut T>;
}

/// owns tasks that access the root application state
pub struct Root<T: 'static> {
    scope: Scope<T>,
}

impl<T: 'static> Root<T> {
    /// runs an async function until it completes or this root is dropped
    pub fn spawn<F>(&mut self, task: F) -> TaskId
    where
        F: for<'a> AsyncFnOnce(ScopeRef<'a, T>) -> () + 'static,
    {
        self.scope.spawn(task)
    }

    /// creates a task scope mapped to state nested within the root application
    pub fn map<U, M>(&self, mapper: &'static M) -> Scope<U>
    where
        U: 'static,
        M: for<'a> Fn(&'a mut T) -> Option<&'a mut U>,
    {
        scope::mapped(&self.scope, mapper)
    }

    /// creates a task scope projected from the root application
    pub fn project<U>(&self) -> Scope<U>
    where
        U: 'static,
        T: Project<U>,
    {
        self.map(&T::project)
    }

    /// cancels a task owned by this root
    pub fn cancel(&mut self, id: TaskId) -> bool {
        self.scope.cancel(id)
    }
}

#[must_not_suspend = "mutable app access cannot be held across a suspend point"]
/// exclusive mutable application access during a task poll
pub struct AppMut<'a, A: 'static> {
    executor: Rc<ExecutorCore>,
    root: NonNull<()>,
    app: NonNull<A>,
    borrow: PhantomData<&'a mut A>,
}

/// a local task queue driven by a platform event loop
pub struct LocalExecutor<A: 'static> {
    core: Rc<ExecutorCore>,
    app: PhantomData<fn(&mut A)>,
}

impl<A: 'static> LocalExecutor<A> {
    /// creates an executor that wakes the platform event loop when tasks are ready
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            core: Rc::new(ExecutorCore {
                tasks: task::TaskExecutor::new(wake),
                access: Cell::new(AppAccess::Inactive),
            }),
            app: PhantomData,
        }
    }

    /// creates the root task scope for this executor
    pub fn root(&self) -> Root<A> {
        Root {
            scope: scope::identity(Rc::downgrade(&self.core)),
        }
    }

    /// polls ready tasks with temporary access to `app`
    pub fn run_ready(&self, app: &mut A) -> bool {
        struct ResetAppAccess<'a>(&'a ExecutorCore);

        impl Drop for ResetAppAccess<'_> {
            fn drop(&mut self) {
                self.0.access.set(AppAccess::Inactive);
            }
        }

        let executor = &self.core;
        assert!(
            matches!(executor.access.get(), AppAccess::Inactive),
            "tasks already running"
        );
        executor
            .access
            .set(AppAccess::Available(NonNull::from(app).cast()));
        let _reset_app_access = ResetAppAccess(executor);
        executor.tasks.run_ready(|task| {
            if let AppAccess::Borrowed(location) = executor.access.get() {
                // drop the task before panicking so app access cannot survive a caught panic
                executor.tasks.cancel(task);
                panic!(
                    "app borrowed at {location} across await. add these attributes to your crate:\n\
                     #![feature(must_not_suspend)]\n\
                     #![deny(must_not_suspend)]"
                )
            }
        })
    }
}

struct ExecutorCore {
    tasks: task::TaskExecutor,
    access: Cell<AppAccess>,
}

#[derive(Clone, Copy)]
enum AppAccess {
    Inactive,
    Available(NonNull<()>),
    Borrowed(&'static Location<'static>),
}

impl<A> Deref for AppMut<'_, A> {
    type Target = A;

    fn deref(&self) -> &Self::Target {
        // safety: taking the app slot guarantees exclusive access during task polling
        unsafe { self.app.as_ref() }
    }
}

impl<A> DerefMut for AppMut<'_, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // safety: taking the app slot guarantees exclusive access during task polling
        unsafe { self.app.as_mut() }
    }
}

impl<A> Drop for AppMut<'_, A> {
    fn drop(&mut self) {
        self.executor.access.set(AppAccess::Available(self.root));
    }
}
