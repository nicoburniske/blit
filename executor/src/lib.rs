#![cfg_attr(feature = "nightly", feature(must_not_suspend))]
#![cfg_attr(feature = "nightly", deny(must_not_suspend))]
//! local task execution for blit platform runners

use std::{
    cell::Cell,
    marker::PhantomData,
    panic::Location,
    ptr::NonNull,
    rc::Rc,
};

mod scope;
mod task;

pub use self::{
    scope::{Borrow, Project, TaskCx, TaskScope},
    task::TaskId,
};

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
    pub fn root(&self) -> TaskScope<A> {
        scope::root(Rc::downgrade(&self.core))
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
        executor.access.set(AppAccess::Available(NonNull::from(app).cast()));
        let _reset_app_access = ResetAppAccess(executor);
        executor.tasks.run_ready(|task| {
            if let AppAccess::Borrowed(location) = executor.access.get() {
                // drop the task before panicking so app access cannot survive a caught panic
                executor.tasks.remove(task);
                panic!(
                    "state borrowed at {location} across await. drop the borrow before awaiting \
                     (enable blit-executor's `nightly` feature for compile-time must_not_suspend checks)"
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
