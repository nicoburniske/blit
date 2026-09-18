use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use slotmap::{SlotMap, new_key_type};

new_key_type! {
    /// identifies a task within a local executor
    pub struct TaskId;
}

pub struct TaskExecutor {
    tasks: RefCell<SlotMap<TaskId, Task>>,
    ready: Arc<ReadyQueue>,
    batch: RefCell<VecDeque<TaskId>>,
}

impl TaskExecutor {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            tasks: RefCell::new(SlotMap::with_key()),
            ready: Arc::new(ReadyQueue {
                tasks: Mutex::new(VecDeque::new()),
                wake: Box::new(wake),
            }),
            batch: RefCell::new(VecDeque::new()),
        }
    }

    pub fn spawn(&self, future: impl Future<Output = ()> + 'static) -> TaskId {
        let mut tasks = self.tasks.borrow_mut();
        let id = tasks.insert_with_key(|id| Task {
            future: Some(Box::pin(future)),
            wake: Arc::new(TaskWake {
                id,
                queued: AtomicBool::new(false),
                ready: self.ready.clone(),
            }),
        });
        let wake = tasks[id].wake.clone();
        drop(tasks);
        wake.notify();
        id
    }

    pub fn run_ready(&self, mut polled: impl FnMut(TaskId)) -> bool {
        let mut batch = self.batch.borrow_mut();
        {
            let mut ready = self.ready.tasks.lock().unwrap();
            // wakes during polling remain queued for the next run
            std::mem::swap(&mut *batch, &mut ready);
        }
        let mut ran = false;
        while let Some(id) = batch.pop_front() {
            let Some((mut future, wake)) = ({
                let mut tasks = self.tasks.borrow_mut();
                tasks.get_mut(id).map(|task| {
                    (
                        task.future.take().expect("task already running"),
                        task.wake.clone(),
                    )
                })
            }) else {
                continue;
            };
            wake.queued.swap(false, Ordering::Acquire);
            let waker = Waker::from(wake);
            let mut context = Context::from_waker(&waker);
            match future.as_mut().poll(&mut context) {
                Poll::Ready(()) => {
                    self.cancel(id);
                }
                Poll::Pending => {
                    let mut tasks = self.tasks.borrow_mut();
                    if let Some(task) = tasks.get_mut(id) {
                        task.future = Some(future);
                    }
                }
            }
            ran = true;
            polled(id);
        }
        ran
    }

    pub fn cancel(&self, id: TaskId) {
        let task = self.tasks.borrow_mut().remove(id);
        drop(task);
    }

    pub fn contains(&self, id: TaskId) -> bool {
        self.tasks.borrow().contains_key(id)
    }
}

struct Task {
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    wake: Arc<TaskWake>,
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
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::Poll,
    };

    use super::*;

    #[test]
    fn ready_queue_skips_canceled_tasks_and_coalesces_wakes() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let executor = TaskExecutor::new({
            let wakes = wakes.clone();
            move || {
                wakes.fetch_add(1, Ordering::Relaxed);
            }
        });
        let canceled = executor.spawn(pending::<()>());
        executor.cancel(canceled);
        let polls = Arc::new(AtomicUsize::new(0));
        let stored = Arc::new(Mutex::new(None));
        let task = executor.spawn(poll_fn({
            let polls = polls.clone();
            let stored = stored.clone();
            move |context| {
                if polls.fetch_add(1, Ordering::Relaxed) == 1 {
                    context.waker().wake_by_ref();
                }
                *stored.lock().unwrap() = Some(context.waker().clone());
                Poll::Pending
            }
        }));

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
        executor.cancel(task);
    }
}
