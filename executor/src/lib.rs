#![feature(must_not_suspend)]
#![deny(must_not_suspend)]
//! local task execution for Blit platform runners

use std::{
    cell::Cell,
    future::Future,
    marker::{PhantomData, PhantomPinned},
    ops::{Deref, DerefMut},
    panic::Location,
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
};

mod scope;
mod task;

pub use scope::{Scope, ScopeRef};
pub use task::{TaskHandle, TaskId};

/// operations available to futures running on the application thread
pub struct Ops<A: 'static> {
    executor: NonNull<ExecutorCore>,
    local: PhantomData<(Rc<()>, fn(&mut A))>,
}

impl<A> Copy for Ops<A> {}

impl<A> Clone for Ops<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: 'static> Ops<A> {
    /// creates a task scope mapped to state nested within the root application
    pub fn map<T, M>(&self, mapper: &'static M) -> Scope<T>
    where
        T: 'static,
        M: for<'a> Fn(&'a mut A) -> Option<&'a mut T>,
    {
        scope::new(self.executor, mapper)
    }

    /// schedules a `!Send` future on the local application thread
    pub fn spawn<F>(&self, future: F) -> TaskHandle
    where
        F: Future<Output = ()> + 'static,
    {
        self.executor().tasks.spawn(future)
    }

    #[track_caller]
    /// provides mutable application access for the remainder of the current task poll
    ///
    /// the returned handle must be dropped before awaiting another future
    pub fn app(&self) -> AppMut<A> {
        borrow_app(self.executor, |root| Some(root.cast()))
    }
}

#[must_not_suspend = "mutable app access cannot be held across a suspend point"]
/// exclusive mutable application access during a task poll
pub struct AppMut<A: 'static> {
    ops: Ops<A>,
    app: NonNull<A>,
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

    /// creates a typed handle into this pinned executor
    ///
    /// # Safety
    ///
    /// the executor must remain pinned and alive whenever the returned handle can be used
    pub unsafe fn ops(self: Pin<&Self>) -> Ops<A> {
        Ops {
            executor: NonNull::from(&self.get_ref().core),
            local: PhantomData,
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

impl<A: 'static> Ops<A> {
    fn from_executor(executor: NonNull<ExecutorCore>) -> Self {
        Self {
            executor,
            local: PhantomData,
        }
    }

    fn executor(&self) -> &ExecutorCore {
        // safety: the platform keeps the pinned executor alive while Ops can be used
        unsafe { self.executor.as_ref() }
    }
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
        self.ops.executor().borrowed_at.set(None);
    }
}

#[track_caller]
fn borrow_app<A>(
    executor: NonNull<ExecutorCore>,
    access: impl FnOnce(NonNull<()>) -> Option<NonNull<A>>,
) -> AppMut<A> {
    // safety: handles cannot outlive the pinned executor that created them
    let executor_ref = unsafe { executor.as_ref() };
    let Some(root) = executor_ref.root.get() else {
        panic!("application access outside task poll");
    };
    if let Some(location) = executor_ref.borrowed_at.get() {
        panic!("application already borrowed at {location}");
    }
    executor_ref.borrowed_at.set(Some(Location::caller()));
    let Some(app) = access(root) else {
        executor_ref.borrowed_at.set(None);
        panic!("scoped application state unavailable");
    };
    AppMut {
        ops: Ops::from_executor(executor),
        app,
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::AppMut;

    #[test]
    fn app_mut_is_a_two_word_handle() {
        assert_eq!(size_of::<AppMut<()>>(), size_of::<usize>() * 2);
    }
}
