use std::{
    cell::{Cell, RefCell, RefMut},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use blit_arena::{Arena, Index};

use crate::ExecutorCore;

/// identifies a task within a local executor
pub type TaskId = Index<Task>;

pub struct TaskExecutor {
    tasks: RefCell<Arena<Task>>,
    ready: Arc<ReadyQueue>,
    batch: RefCell<VecDeque<TaskId>>,
}

impl TaskExecutor {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            tasks: RefCell::new(Arena::new()),
            ready: Arc::new(ReadyQueue {
                tasks: Mutex::new(VecDeque::new()),
                wake: Box::new(wake),
            }),
            batch: RefCell::new(VecDeque::new()),
        }
    }

    pub fn spawn(&self, scope: Rc<dyn TaskListState>, future: impl Future<Output = ()> + 'static) -> TaskId {
        let mut tasks = self.tasks.borrow_mut();
        let next = scope.tasks().head.get();
        let id = tasks.insert_with(|id| Task {
            future: Some(Box::pin(future)),
            wake: Arc::new(TaskWake {
                id,
                queued: AtomicBool::new(false),
                ready: self.ready.clone(),
            }),
            scope,
            previous: None,
            next,
        });
        if let Some(next) = next {
            tasks.get_mut(next).unwrap().previous = Some(id);
        }
        let task = tasks.get(id).unwrap();
        task.scope.tasks().head.set(Some(id));
        let wake = task.wake.clone();
        drop(tasks);
        wake.notify();
        id
    }

    pub fn run_ready(&self, mut polled: impl FnMut(TaskId)) -> bool {
        struct ReadyBatch<'a> {
            executor: &'a TaskExecutor,
            tasks: RefMut<'a, VecDeque<TaskId>>,
            current: Option<TaskId>,
        }

        impl Drop for ReadyBatch<'_> {
            fn drop(&mut self) {
                if let Some(id) = self.current {
                    self.executor.remove(id);
                }
                if !self.tasks.is_empty() {
                    (self.executor.ready.wake)();
                }
            }
        }

        let mut batch = ReadyBatch {
            executor: self,
            tasks: self.batch.borrow_mut(),
            current: None,
        };
        {
            let mut ready = self.ready.tasks.lock().unwrap();
            // wakes during polling remain queued for the next run
            if batch.tasks.is_empty() {
                std::mem::swap(&mut *batch.tasks, &mut ready);
            } else {
                batch.tasks.append(&mut ready);
            }
        }
        let mut ran = false;
        while let Some(id) = batch.tasks.pop_front() {
            let Some((mut future, wake)) = ({
                let mut tasks = self.tasks.borrow_mut();
                tasks
                    .get_mut(id)
                    .map(|task| (task.future.take().expect("task already running"), task.wake.clone()))
            }) else {
                continue;
            };
            batch.current = Some(id);
            wake.queued.swap(false, Ordering::Acquire);
            let waker = Waker::from(wake);
            let mut context = Context::from_waker(&waker);
            match future.as_mut().poll(&mut context) {
                Poll::Ready(()) => {
                    self.remove(id);
                }
                Poll::Pending => {
                    let mut tasks = self.tasks.borrow_mut();
                    if let Some(task) = tasks.get_mut(id) {
                        task.future = Some(future);
                    }
                }
            }
            batch.current = None;
            ran = true;
            polled(id);
        }
        ran
    }

    pub fn cancel<S: ?Sized>(&self, id: TaskId, scope: &Rc<S>) -> bool {
        let owned = self
            .tasks
            .borrow()
            .get(id)
            .is_some_and(|task| std::ptr::addr_eq(Rc::as_ptr(&task.scope), Rc::as_ptr(scope)));
        owned && self.remove(id)
    }

    pub fn remove(&self, id: TaskId) -> bool {
        let mut tasks = self.tasks.borrow_mut();
        let Some(task) = tasks.remove(id) else {
            return false;
        };
        if let Some(previous) = task.previous {
            tasks.get_mut(previous).unwrap().next = task.next;
        } else {
            task.scope.tasks().head.set(task.next);
        }
        if let Some(next) = task.next {
            tasks.get_mut(next).unwrap().previous = task.previous;
        }
        // dropping the future may cancel other tasks
        drop(tasks);
        drop(task);
        true
    }
}

pub struct Task {
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    wake: Arc<TaskWake>,
    scope: Rc<dyn TaskListState>,
    previous: Option<TaskId>,
    next: Option<TaskId>,
}

#[derive(Default)]
pub struct TaskList {
    pub executor: Weak<ExecutorCore>,
    pub head: Cell<Option<TaskId>>,
}

pub trait TaskListState {
    fn tasks(&self) -> &TaskList;
}

struct TaskWake {
    id: TaskId,
    queued: AtomicBool,
    ready: Arc<ReadyQueue>,
}

struct ReadyQueue {
    tasks: Mutex<VecDeque<TaskId>>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Wake for TaskWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}

impl TaskWake {
    fn notify(&self) {
        if self.queued.swap(true, Ordering::AcqRel) {
            return;
        }
        let wake = {
            let mut tasks = self.ready.tasks.lock().unwrap();
            let wake = tasks.is_empty();
            tasks.push_back(self.id);
            wake
        };
        if wake {
            (self.ready.wake)();
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        future::{pending, poll_fn},
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::Poll,
    };

    use super::*;

    impl TaskListState for TaskList {
        fn tasks(&self) -> &TaskList {
            self
        }
    }

    #[test]
    fn ready_queue_skips_canceled_tasks_and_coalesces_wakes() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let executor = TaskExecutor::new({
            let wakes = wakes.clone();
            move || {
                wakes.fetch_add(1, Ordering::Relaxed);
            }
        });
        let scope = Rc::new(TaskList::default());
        let canceled = executor.spawn(scope.clone(), pending::<()>());
        executor.cancel(canceled, &scope);
        let polls = Arc::new(AtomicUsize::new(0));
        let stored = Arc::new(Mutex::new(None));
        let task = executor.spawn(
            scope.clone(),
            poll_fn({
                let polls = polls.clone();
                let stored = stored.clone();
                move |context| {
                    if polls.fetch_add(1, Ordering::Relaxed) == 1 {
                        context.waker().wake_by_ref();
                    }
                    *stored.lock().unwrap() = Some(context.waker().clone());
                    Poll::Pending
                }
            }),
        );

        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        assert!(executor.run_ready(|_| {}));
        assert_eq!(polls.load(Ordering::Relaxed), 1);
        let waker = stored.lock().unwrap().take().unwrap();
        waker.wake_by_ref();
        waker.wake_by_ref();
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        assert!(executor.run_ready(|_| {}));
        assert_eq!(polls.load(Ordering::Relaxed), 2);
        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        assert!(executor.run_ready(|_| {}));
        assert_eq!(polls.load(Ordering::Relaxed), 3);
        executor.cancel(task, &scope);
        let replacement = executor.spawn(scope.clone(), pending::<()>());
        assert_ne!(replacement, task);
        assert!(executor.run_ready(|id| assert_eq!(id, replacement)));
        waker.wake_by_ref();
        assert!(!executor.run_ready(|_| panic!("canceled task woke replacement")));
        executor.cancel(replacement, &scope);
    }

    #[test]
    fn panicking_tasks_are_removed_and_remaining_tasks_still_run() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let executor = TaskExecutor::new({
            let wakes = wakes.clone();
            move || {
                wakes.fetch_add(1, Ordering::Relaxed);
            }
        });
        let scope = Rc::new(TaskList::default());
        let panicking = executor.spawn(scope.clone(), async { panic!("task panicked") });
        let remaining = executor.spawn(scope.clone(), pending::<()>());
        let last = executor.spawn(scope.clone(), pending::<()>());

        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        assert!(catch_unwind(AssertUnwindSafe(|| executor.run_ready(|_| {}))).is_err());
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        assert!(!executor.cancel(panicking, &scope));
        assert!(
            catch_unwind(AssertUnwindSafe(|| executor.run_ready(|id| {
                assert_eq!(id, remaining);
                panic!("post poll check failed");
            })))
            .is_err()
        );
        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        assert!(executor.run_ready(|id| assert_eq!(id, last)));
        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        assert!(executor.cancel(remaining, &scope));
        assert!(executor.cancel(last, &scope));
    }
}
