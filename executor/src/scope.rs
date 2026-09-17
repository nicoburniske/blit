use std::{
    ops::AsyncFnOnce,
    panic::Location,
    ptr::NonNull,
    rc::{Rc, Weak},
};

use crate::{AppAccess, AppMut, ExecutorCore, TaskId};

/// owns tasks that access state mapped from the root application
pub struct Scope<T: 'static> {
    handle: ScopeHandle<T>,
    tasks: Vec<TaskId>,
}

/// state access available only within a scoped async function
pub struct ScopeRef<'a, T: 'static> {
    handle: &'a ScopeHandle<T>,
}

impl<T: 'static> Scope<T> {
    /// runs an async function until it completes or this scope is dropped
    pub fn spawn<F>(&mut self, task: F) -> TaskId
    where
        F: for<'a> AsyncFnOnce(ScopeRef<'a, T>) -> () + 'static,
    {
        let executor = self.handle.executor();
        self.tasks.retain(|task| executor.tasks.contains(*task));
        let handle = self.handle.clone();
        let id = executor.tasks.spawn(async move {
            task(ScopeRef { handle: &handle }).await;
        });
        self.tasks.push(id);
        id
    }

    /// cancels a task owned by this scope
    pub fn cancel(&mut self, id: TaskId) -> bool {
        let Some(index) = self.tasks.iter().position(|task| *task == id) else {
            return false;
        };
        self.tasks.swap_remove(index);
        if let Some(executor) = self.handle.executor.upgrade() {
            executor.tasks.cancel(id);
        }
        true
    }
}

impl<T: 'static> Drop for Scope<T> {
    fn drop(&mut self) {
        let Some(executor) = self.handle.executor.upgrade() else {
            return;
        };
        for task in self.tasks.drain(..) {
            executor.tasks.cancel(task);
        }
    }
}

impl<T: 'static> ScopeRef<'_, T> {
    #[track_caller]
    /// provides mutable mapped state access for the remainder of the current task poll
    pub fn app(&self) -> AppMut<'_, T> {
        let executor = self.handle.executor();
        let root = match executor.access.get() {
            AppAccess::Available(root) => root,
            AppAccess::Borrowed(location) => {
                panic!("application already borrowed at {location}")
            }
            AppAccess::Inactive => panic!("application access outside task poll"),
        };
        executor.access.set(AppAccess::Borrowed(Location::caller()));
        // safety: `map` pairs this function with its static mapper and root type
        let Some(app) = (unsafe { (self.handle.access)(self.handle.mapper, root) }) else {
            executor.access.set(AppAccess::Available(root));
            panic!("scoped application state unavailable");
        };
        AppMut {
            executor,
            root,
            app,
            borrow: std::marker::PhantomData,
        }
    }
}

//
// internal
//

pub fn identity<T: 'static>(executor: Weak<ExecutorCore>) -> Scope<T> {
    Scope {
        handle: ScopeHandle {
            executor,
            mapper: std::ptr::null(),
            access: |_, app| Some(app.cast()),
        },
        tasks: Vec::new(),
    }
}

pub fn mapped<A, T, M>(root: &Scope<A>, mapper: &'static M) -> Scope<T>
where
    A: 'static,
    T: 'static,
    M: for<'a> Fn(&'a mut A) -> Option<&'a mut T>,
{
    Scope {
        handle: ScopeHandle {
            executor: root.handle.executor.clone(),
            mapper: mapper as *const M as *const (),
            access: mapped_access::<A, T, M>,
        },
        tasks: Vec::new(),
    }
}

struct ScopeHandle<T: 'static> {
    executor: Weak<ExecutorCore>,
    mapper: *const (),
    access: unsafe fn(*const (), NonNull<()>) -> Option<NonNull<T>>,
}

impl<T> Clone for ScopeHandle<T> {
    fn clone(&self) -> Self {
        Self {
            executor: self.executor.clone(),
            mapper: self.mapper,
            access: self.access,
        }
    }
}

impl<T: 'static> ScopeHandle<T> {
    fn executor(&self) -> Rc<ExecutorCore> {
        self.executor
            .upgrade()
            .expect("task executor has been dropped")
    }
}

unsafe fn mapped_access<A, T, M>(mapper: *const (), app: NonNull<()>) -> Option<NonNull<T>>
where
    M: for<'a> Fn(&'a mut A) -> Option<&'a mut T>,
{
    let mapper = unsafe { &*mapper.cast::<M>() };
    let app = unsafe { app.cast::<A>().as_mut() };
    mapper(app).map(NonNull::from)
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        future::pending,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    use crate::LocalExecutor;

    use super::*;

    struct App {
        page: Option<Page>,
        count: u32,
    }

    struct Page {
        scope: Scope<Page>,
        count: u32,
    }

    struct DropCount(Rc<Cell<usize>>);

    impl Drop for DropCount {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn scope_accesses_state_and_cancels_tasks() {
        let executor = LocalExecutor::<App>::new(|| {});
        let mut root = executor.root();
        root.spawn(async |cx| {
            cx.app().count += 1;
        });
        let mut scope = root.map(&|app: &mut App| app.page.as_mut());
        let dropped = Rc::new(Cell::new(0));
        let future_dropped = dropped.clone();
        let first = scope.spawn(async move |_| {
            let _drop = DropCount(future_dropped);
            pending::<()>().await;
        });
        let future_dropped = dropped.clone();
        scope.spawn(async move |cx| {
            let _drop = DropCount(future_dropped);
            cx.app().count += 1;
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
    #[allow(must_not_suspend)]
    fn borrow_across_await_drops_task_before_panicking() {
        let executor = LocalExecutor::<App>::new(|| {});
        let mut root = executor.root();
        let dropped = Rc::new(Cell::new(0));
        let future_dropped = dropped.clone();
        root.spawn(async move |cx| {
            let _app = cx.app();
            let _drop = DropCount(future_dropped);
            pending::<()>().await;
        });
        let mut app = App {
            page: None,
            count: 0,
        };

        let panic = catch_unwind(AssertUnwindSafe(|| {
            executor.run_ready(&mut app);
        }));
        assert!(panic.is_err());
        assert_eq!(dropped.get(), 1);
        assert!(!executor.run_ready(&mut app));
    }

    #[test]
    fn escaped_root_rejects_use_after_executor_drop() {
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
}
