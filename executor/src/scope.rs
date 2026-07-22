use std::{marker::PhantomData, ops::AsyncFnOnce, panic::Location, ptr::NonNull, rc::Rc};

use crate::{AppMut, ExecutorCore, TaskId, task::TaskHandle};

/// owns tasks that access state mapped from the root application
pub struct Scope<T: 'static> {
    handle: ScopeHandle<T>,
    tasks: Vec<TaskHandle>,
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
        self.tasks.retain(|task| !task.is_finished());
        let handle = self.handle;
        let task = handle.executor().tasks.spawn(async move {
            task(ScopeRef { handle: &handle }).await;
        });
        let id = task.id();
        self.tasks.push(task);
        id
    }

    /// cancels a task owned by this scope
    pub fn cancel(&mut self, id: TaskId) -> bool {
        let Some(index) = self.tasks.iter().position(|task| task.id() == id) else {
            return false;
        };
        drop(self.tasks.swap_remove(index));
        true
    }
}

impl<T: 'static> ScopeRef<'_, T> {
    #[track_caller]
    /// provides mutable mapped state access for the remainder of the current task poll
    pub fn app(&self) -> AppMut<T> {
        let executor = self.handle.executor();
        let Some(root) = executor.root.get() else {
            panic!("application access outside task poll");
        };
        if let Some(location) = executor.borrowed_at.get() {
            panic!("application already borrowed at {location}");
        }
        executor.borrowed_at.set(Some(Location::caller()));
        let Some(app) = self.handle.access(root) else {
            executor.borrowed_at.set(None);
            panic!("scoped application state unavailable");
        };
        AppMut {
            executor: self.handle.executor,
            app,
            local: PhantomData,
        }
    }
}

//
// internal
//

pub fn identity<T: 'static>(executor: NonNull<ExecutorCore>) -> Scope<T> {
    Scope {
        handle: ScopeHandle {
            executor,
            mapper: std::ptr::null(),
            access: |_, app| Some(app.cast()),
            local: PhantomData,
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
            executor: root.handle.executor,
            mapper: mapper as *const M as *const (),
            access: mapped_access::<A, T, M>,
            local: PhantomData,
        },
        tasks: Vec::new(),
    }
}

struct ScopeHandle<T: 'static> {
    executor: NonNull<ExecutorCore>,
    mapper: *const (),
    access: unsafe fn(*const (), NonNull<()>) -> Option<NonNull<T>>,
    local: PhantomData<Rc<()>>,
}

impl<T> Copy for ScopeHandle<T> {}

impl<T> Clone for ScopeHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> ScopeHandle<T> {
    fn executor(&self) -> &ExecutorCore {
        // safety: the platform keeps the pinned executor alive while scoped tasks can run
        unsafe { self.executor.as_ref() }
    }

    fn access(&self, root: NonNull<()>) -> Option<NonNull<T>> {
        // safety: `map` pairs this function with its static mapper and root type
        unsafe { (self.access)(self.mapper, root) }
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
        collections::VecDeque,
        future::pending,
        rc::Rc,
        sync::{Arc, Mutex},
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

    struct DropFlag(Rc<Cell<bool>>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    #[test]
    fn scope_accesses_state_and_cancels_tasks() {
        let ready = Arc::new(Mutex::new(VecDeque::new()));
        let executor = Box::pin(LocalExecutor::<App>::new({
            let ready = ready.clone();
            move |task| ready.lock().unwrap().push_back(task)
        }));
        let mut root = unsafe { executor.as_ref().root() };
        root.spawn(async |cx| {
            cx.app().count += 1;
        });
        let mut scope = root.map(&|app: &mut App| app.page.as_mut());
        let first_dropped = Rc::new(Cell::new(false));
        let future_dropped = first_dropped.clone();
        let first = scope.spawn(async move |_| {
            let _drop = DropFlag(future_dropped);
            pending::<()>().await;
        });
        let second_dropped = Rc::new(Cell::new(false));
        let future_dropped = second_dropped.clone();
        scope.spawn(async move |cx| {
            let _drop = DropFlag(future_dropped);
            cx.app().count += 1;
            pending::<()>().await;
        });
        let mut app = App {
            page: Some(Page { scope, count: 0 }),
            count: 0,
        };

        let task = ready.lock().unwrap().pop_front().unwrap();
        assert!(executor.as_ref().run(&mut app, task));
        assert_eq!(app.count, 1);

        let task = ready.lock().unwrap().pop_front().unwrap();
        assert_eq!(task, first);
        assert!(executor.as_ref().run(&mut app, task));
        assert!(!first_dropped.get());
        assert!(app.page.as_mut().unwrap().scope.cancel(first));
        assert!(first_dropped.get());
        assert!(!second_dropped.get());

        let task = ready.lock().unwrap().pop_front().unwrap();
        assert!(executor.as_ref().run(&mut app, task));
        assert_eq!(app.page.as_ref().unwrap().count, 1);
        assert!(!second_dropped.get());

        drop(app.page.take());
        assert!(second_dropped.get());
    }
}
