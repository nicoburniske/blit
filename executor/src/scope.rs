use std::{marker::PhantomData, ops::AsyncFnOnce, ptr::NonNull, rc::Rc};

use crate::{AppMut, ExecutorCore, TaskHandle, TaskId, borrow_app};

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
        borrow_app(self.handle.executor, |root| self.handle.access(root))
    }
}

//
// internal
//

pub fn new<A, T, M>(executor: NonNull<ExecutorCore>, mapper: &'static M) -> Scope<T>
where
    A: 'static,
    T: 'static,
    M: for<'a> Fn(&'a mut A) -> Option<&'a mut T>,
{
    Scope {
        handle: ScopeHandle {
            executor,
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

    struct Root {
        page: Option<Page>,
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
        let executor = Box::pin(LocalExecutor::<Root>::new({
            let ready = ready.clone();
            move |task| ready.lock().unwrap().push_back(task)
        }));
        let root_ops = unsafe { executor.as_ref().ops() };
        let mut scope = root_ops.map(&|root: &mut Root| root.page.as_mut());
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
        let mut root = Root {
            page: Some(Page { scope, count: 0 }),
        };

        let task = ready.lock().unwrap().pop_front().unwrap();
        assert_eq!(task, first);
        assert!(executor.as_ref().run(&mut root, task));
        assert!(!first_dropped.get());
        assert!(root.page.as_mut().unwrap().scope.cancel(first));
        assert!(first_dropped.get());
        assert!(!second_dropped.get());

        let task = ready.lock().unwrap().pop_front().unwrap();
        assert!(executor.as_ref().run(&mut root, task));
        assert_eq!(root.page.as_ref().unwrap().count, 1);
        assert!(!second_dropped.get());

        drop(root.page.take());
        assert!(second_dropped.get());
    }
}
