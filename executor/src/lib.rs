#![feature(must_not_suspend)]
#![deny(must_not_suspend)]
//! local task execution for Blit platform runners

use std::{
    cell::Cell,
    marker::{PhantomData, PhantomPinned},
    ops::{AsyncFnOnce, Deref, DerefMut},
    panic::Location,
    pin::Pin,
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
pub struct AppMut<A: 'static> {
    executor: NonNull<ExecutorCore>,
    app: NonNull<A>,
    local: PhantomData<(Rc<()>, fn(&mut A))>,
}

/// a local task queue driven by a platform event loop
pub struct LocalExecutor<A: 'static> {
    core: ExecutorCore,
    app: PhantomData<fn(&mut A)>,
    _pinned: PhantomPinned,
}

impl<A: 'static> LocalExecutor<A> {
    /// creates an executor that sends ready task IDs to the platform event loop
    pub fn new(wake: impl Fn(TaskId) + Send + Sync + 'static) -> Self {
        Self {
            core: ExecutorCore {
                tasks: task::TaskExecutor::new(wake),
                root: Cell::new(None),
                borrowed_at: Cell::new(None),
            },
            app: PhantomData,
            _pinned: PhantomPinned,
        }
    }

    /// creates the root task scope for this pinned executor
    ///
    /// # Safety
    ///
    /// the executor must remain pinned and alive while the returned scope exists
    pub unsafe fn root(self: Pin<&Self>) -> Root<A> {
        Root {
            scope: scope::identity(NonNull::from(&self.get_ref().core)),
        }
    }

    /// polls one ready task with temporary access to `app`
    pub fn run(self: Pin<&Self>, app: &mut A, task: TaskId) -> bool {
        struct ResetAppAccess<'a>(&'a ExecutorCore);

        impl Drop for ResetAppAccess<'_> {
            fn drop(&mut self) {
                self.0.root.set(None);
                self.0.borrowed_at.set(None);
            }
        }

        let executor = &self.get_ref().core;
        assert!(executor.root.get().is_none(), "tasks already running");
        assert!(
            executor.borrowed_at.get().is_none(),
            "application borrow state lost"
        );
        executor.root.set(Some(NonNull::from(app).cast()));
        let _reset_app_access = ResetAppAccess(executor);
        let ran = executor.tasks.run(task);

        if let Some(location) = executor.borrowed_at.get() {
            panic!(
                "app borrowed at {location} across await. add these attributes to your crate:\n\
                 #![feature(must_not_suspend)]\n\
                 #![deny(must_not_suspend)]"
            )
        }
        ran
    }
}

struct ExecutorCore {
    tasks: task::TaskExecutor,
    root: Cell<Option<NonNull<()>>>,
    borrowed_at: Cell<Option<&'static Location<'static>>>,
}

impl<A> Deref for AppMut<A> {
    type Target = A;

    fn deref(&self) -> &Self::Target {
        // safety: taking the app slot guarantees exclusive access during task polling
        unsafe { self.app.as_ref() }
    }
}

impl<A> DerefMut for AppMut<A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // safety: taking the app slot guarantees exclusive access during task polling
        unsafe { self.app.as_mut() }
    }
}

impl<A> Drop for AppMut<A> {
    fn drop(&mut self) {
        // safety: the task scope cannot outlive its pinned executor
        unsafe { self.executor.as_ref() }.borrowed_at.set(None);
    }
}
