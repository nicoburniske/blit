use std::{
    ops::{AsyncFnOnce, Deref, DerefMut},
    panic::Location,
    ptr::NonNull,
    rc::{Rc, Weak},
};

use crate::{
    AppAccess, ExecutorCore, TaskId,
    task::{TaskList, TaskListState},
};

/// owns tasks that access state mapped from the application
pub struct TaskScope<T: 'static> {
    state: Rc<dyn ScopeState<T>>,
}

impl<T: 'static> TaskScope<T> {
    /// runs an async function until it completes or this scope is dropped
    pub fn spawn<F>(&mut self, task: F) -> TaskId
    where
        F: for<'a> AsyncFnOnce(TaskCx<'a, T>) -> () + 'static,
    {
        let tasks = self.state.tasks();
        let executor = tasks.executor.upgrade().expect("task executor has been dropped");
        let state = self.state.clone();
        executor.tasks.spawn(self.state.clone(), async move {
            task(TaskCx { state: &*state }).await;
        })
    }

    /// creates an independent task scope mapped from this scope's state
    ///
    /// mappings are resolved on each state access and survive this scope being dropped
    pub fn map<U, M>(&self, mapper: M) -> TaskScope<U>
    where
        U: 'static,
        M: for<'a> Fn(&'a mut T) -> Option<&'a mut U> + 'static,
    {
        let parent = self.state.clone();
        TaskScope {
            state: Rc::new(ScopeData {
                tasks: TaskList {
                    executor: self.state.tasks().executor.clone(),
                    ..TaskList::default()
                },
                projection: move |root| {
                    let mut app = parent.project(root)?;
                    // safety: the parent projects this scope's state from the exclusively borrowed app
                    mapper(unsafe { app.as_mut() }).map(NonNull::from)
                },
            }),
        }
    }

    /// creates an independent task scope projected from this scope's state
    pub fn project<U>(&self) -> TaskScope<U>
    where
        U: 'static,
        T: Project<U>,
    {
        self.map(T::project)
    }

    /// returns whether a live task owned by this scope was canceled
    pub fn cancel(&mut self, id: TaskId) -> bool {
        let Some(executor) = self.state.tasks().executor.upgrade() else {
            return false;
        };
        executor.tasks.cancel(id, &self.state)
    }

    /// cancels all live tasks owned by this scope
    pub fn clear(&mut self) {
        let Some(executor) = self.state.tasks().executor.upgrade() else {
            return;
        };
        while let Some(task) = self.state.tasks().head.get() {
            executor.tasks.remove(task);
        }
    }
}

/// maps application state to nested state when it is available
pub trait Project<T: 'static> {
    fn project(&mut self) -> Option<&mut T>;
}

/// state access available only within a scoped async function
pub struct TaskCx<'a, T: 'static> {
    state: &'a dyn ScopeState<T>,
}

impl<T: 'static> Drop for TaskScope<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T: 'static> TaskCx<'_, T> {
    #[track_caller]
    /// provides mutable mapped state access for the remainder of the current task poll
    pub fn borrow(&self) -> Borrow<'_, T> {
        let tasks = self.state.tasks();
        let executor = tasks.executor.upgrade().expect("task executor has been dropped");
        let root = match executor.access.get() {
            AppAccess::Available(root) => root,
            AppAccess::Borrowed(location) => {
                panic!("application already borrowed at {location}")
            }
            AppAccess::Inactive => panic!("application access outside task poll"),
        };
        executor.access.set(AppAccess::Borrowed(Location::caller()));
        let borrow = AppBorrow { executor, root };
        let Some(mut app) = self.state.project(root) else {
            panic!("scoped application state unavailable");
        };
        Borrow {
            // safety: the borrow guard holds exclusive app access until drop
            app: unsafe { app.as_mut() },
            _borrow: borrow,
        }
    }
}

#[cfg_attr(
    feature = "nightly",
    must_not_suspend = "mutable state borrow cannot be held across a suspend point"
)]
/// exclusive mutable state access during a task poll
pub struct Borrow<'a, T: 'static> {
    app: &'a mut T,
    _borrow: AppBorrow,
}

impl<T> Deref for Borrow<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.app
    }
}

impl<T> DerefMut for Borrow<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.app
    }
}

struct AppBorrow {
    executor: Rc<ExecutorCore>,
    root: NonNull<()>,
}

impl Drop for AppBorrow {
    fn drop(&mut self) {
        self.executor.access.set(AppAccess::Available(self.root));
    }
}

pub fn root<T: 'static>(executor: Weak<ExecutorCore>) -> TaskScope<T> {
    TaskScope {
        state: Rc::new(ScopeData {
            tasks: TaskList {
                executor,
                ..TaskList::default()
            },
            projection: |app: NonNull<()>| Some(app.cast()),
        }),
    }
}

trait ScopeState<T>: TaskListState {
    fn project(&self, root: NonNull<()>) -> Option<NonNull<T>>;
}

struct ScopeData<P> {
    tasks: TaskList,
    projection: P,
}

impl<T, P> ScopeState<T> for ScopeData<P>
where
    P: Fn(NonNull<()>) -> Option<NonNull<T>>,
{
    fn project(&self, root: NonNull<()>) -> Option<NonNull<T>> {
        (self.projection)(root)
    }
}

impl<P> TaskListState for ScopeData<P> {
    fn tasks(&self) -> &TaskList {
        &self.tasks
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        future::pending,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    use super::*;
    use crate::LocalExecutor;

    struct App {
        page: Option<Page>,
        count: u32,
    }

    struct Page {
        scope: TaskScope<Page>,
        count: u32,
    }

    struct DropCount(Rc<Cell<usize>>);

    impl Drop for DropCount {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    impl Project<u32> for Option<u32> {
        fn project(&mut self) -> Option<&mut u32> {
            self.as_mut()
        }
    }

    #[test]
    fn scope_accesses_state_and_cancels_tasks() {
        let executor = LocalExecutor::<App>::new(|| {});
        let mut root = executor.root();
        root.spawn(async |cx| {
            cx.borrow().count += 1;
        });
        let mut scope = root.map(|app: &mut App| app.page.as_mut());
        let dropped = Rc::new(Cell::new(0));
        let future_dropped = dropped.clone();
        let first = scope.spawn(async move |_| {
            let _drop = DropCount(future_dropped);
            pending::<()>().await;
        });
        let future_dropped = dropped.clone();
        scope.spawn(async move |cx| {
            let _drop = DropCount(future_dropped);
            cx.borrow().count += 1;
            pending::<()>().await;
        });
        let mut app = App {
            page: Some(Page { scope, count: 0 }),
            count: 0,
        };

        assert!(executor.run_ready(&mut app));
        assert_eq!(app.count, 1);
        assert_eq!(app.page.as_ref().unwrap().count, 1);
        assert_eq!(dropped.get(), 0);

        assert!(app.page.as_mut().unwrap().scope.cancel(first));
        assert_eq!(dropped.get(), 1);

        drop(app.page.take());
        assert_eq!(dropped.get(), 2);
    }

    #[test]
    fn mapped_scopes_compose_and_outlive_parent_scopes() {
        let executor = LocalExecutor::<Option<Option<u32>>>::new(|| {});
        let root = executor.root();
        let mut page = root.map(Option::as_mut);
        let mut count = page.project::<u32>();
        for _ in 0..16 {
            count = count.map(|count: &mut u32| Some(count));
        }
        page.spawn(async |_| panic!("dropped parent task was polled"));
        drop(page);
        drop(root);
        for initial in [0, 10] {
            let mut app = Some(Some(initial));
            count.spawn(async |cx| {
                *cx.borrow() += 1;
            });
            assert!(executor.run_ready(&mut app));
            assert_eq!(app, Some(Some(initial + 1)));
        }
    }

    #[test]
    fn missing_mapped_state_releases_app_access() {
        let executor = LocalExecutor::<Option<Option<u32>>>::new(|| {});
        let mut root = executor.root();
        let parent = root.map(Option::as_mut);
        let mut child = parent.map(Option::as_mut);

        for mut app in [None, Some(None)] {
            child.spawn(async |cx| {
                *cx.borrow() += 1;
            });
            root.spawn(async |cx| {
                *cx.borrow() = Some(Some(7));
            });

            let error = catch_unwind(AssertUnwindSafe(|| executor.run_ready(&mut app))).unwrap_err();
            assert_eq!(
                error.downcast_ref::<&str>(),
                Some(&"scoped application state unavailable")
            );
            assert!(executor.run_ready(&mut app));
            assert_eq!(app, Some(Some(7)));
        }
    }

    #[test]
    fn caught_mapper_panic_releases_app_access() {
        let executor = LocalExecutor::<u32>::new(|| {});
        let root = executor.root();
        let mut scope = root.map(|app: &mut u32| {
            if *app == 0 {
                *app = 1;
                panic!("mapper failed");
            }
            Some(app)
        });
        scope.spawn(async |cx| {
            assert!(catch_unwind(AssertUnwindSafe(|| drop(cx.borrow()))).is_err());
            *cx.borrow() += 1;
        });
        let mut app = 0;

        assert!(executor.run_ready(&mut app));
        assert_eq!(app, 2);
    }

    #[test]
    #[cfg_attr(feature = "nightly", allow(must_not_suspend))]
    fn borrow_across_await_drops_task_before_panicking() {
        let executor = LocalExecutor::<App>::new(|| {});
        let mut root = executor.root();
        let dropped = Rc::new(Cell::new(0));
        let future_dropped = dropped.clone();
        root.spawn(async move |cx| {
            let _app = cx.borrow();
            let _drop = DropCount(future_dropped);
            pending::<()>().await;
        });
        let mut app = App { page: None, count: 0 };

        let panic = catch_unwind(AssertUnwindSafe(|| {
            executor.run_ready(&mut app);
        }));
        assert!(panic.is_err());
        assert_eq!(dropped.get(), 1);
        assert!(!executor.run_ready(&mut app));
    }

    #[test]
    fn escaped_scope_rejects_use_after_executor_drop() {
        let executor = LocalExecutor::<App>::new(|| {});
        let mut root = executor.root();
        let dropped = Rc::new(Cell::new(0));
        let future_drop = DropCount(dropped.clone());
        root.spawn(async move |_| {
            let _drop = future_drop;
            pending::<()>().await;
        });

        drop(executor);
        assert_eq!(dropped.get(), 1);
        let panic = catch_unwind(AssertUnwindSafe(|| root.spawn(async |_| {})));
        assert!(panic.is_err());
    }

    #[test]
    fn cancellation_only_affects_live_tasks_in_the_same_scope() {
        let executor = LocalExecutor::<()>::new(|| {});
        let mut root = executor.root();
        let mut other = executor.root();
        let completed = root.spawn(async |_| {});
        let first = root.spawn(async |_| pending::<()>().await);

        assert!(!other.cancel(first));
        assert!(root.cancel(first));
        assert!(!root.cancel(first));
        assert!(executor.run_ready(&mut ()));
        assert!(!root.cancel(completed));
        let replacement = root.spawn(async |_| pending::<()>().await);
        assert_ne!(replacement, completed);
        assert!(!root.cancel(completed));
        drop(root);
        assert!(!executor.run_ready(&mut ()));
    }

    #[test]
    fn a_running_task_can_drop_its_own_scope() {
        struct App {
            scope: Option<TaskScope<App>>,
        }

        let executor = LocalExecutor::<App>::new(|| {});
        let mut root = executor.root();
        let dropped = Rc::new(Cell::new(0));
        let task_drop = DropCount(dropped.clone());
        root.spawn(async move |cx| {
            let _drop = task_drop;
            drop(cx.borrow().scope.take());
            pending::<()>().await;
        });
        let mut nested = executor.root();
        let nested_drop = DropCount(dropped.clone());
        nested.spawn(async move |_| {
            let _drop = nested_drop;
            pending::<()>().await;
        });
        root.spawn(async move |_| {
            let _nested = nested;
            pending::<()>().await;
        });
        let mut app = App { scope: Some(root) };

        assert!(executor.run_ready(&mut app));
        assert!(app.scope.is_none());
        assert_eq!(dropped.get(), 2);
        assert!(!executor.run_ready(&mut app));
    }
}
