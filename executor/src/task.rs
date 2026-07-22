use std::{
    cell::RefCell,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

use slotmap::{SlotMap, new_key_type};

new_key_type! {
    /// identifies a task within a local executor
    pub struct TaskId;
}

#[must_use = "dropping this handle cancels the task"]
/// a local task that is cancelled when dropped
pub struct TaskHandle {
    executor: NonNull<TaskExecutor>,
    id: TaskId,
    cancel_on_drop: bool,
    local: PhantomData<Rc<()>>,
}

impl TaskHandle {
    pub fn detach(mut self) {
        self.cancel_on_drop = false;
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn is_finished(&self) -> bool {
        task_finished(self.executor, self.id)
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            cancel_task(self.executor, self.id);
        }
    }
}

pub struct TaskExecutor {
    tasks: RefCell<SlotMap<TaskId, Task>>,
    wake: Arc<dyn Fn(TaskId) + Send + Sync>,
}

impl TaskExecutor {
    pub fn new(wake: impl Fn(TaskId) + Send + Sync + 'static) -> Self {
        Self {
            tasks: RefCell::new(SlotMap::with_key()),
            wake: Arc::new(wake),
        }
    }

    pub fn spawn(&self, future: impl Future<Output = ()> + 'static) -> TaskHandle {
        let mut tasks = self.tasks.borrow_mut();
        let id = tasks.insert_with_key(|id| Task {
            future: Some(Box::pin(future)),
            wake: TaskWake {
                id,
                wake: self.wake.clone(),
            },
        });
        let wake = tasks[id].wake.clone();
        drop(tasks);
        wake.notify();
        TaskHandle {
            executor: NonNull::from(self),
            id,
            cancel_on_drop: true,
            local: PhantomData,
        }
    }

    pub fn run(&self, id: TaskId) -> bool {
        let Some(ready) = self.take_ready(id) else {
            return false;
        };
        let ReadyTask { mut future, wake } = ready;
        let waker = Waker::from(Arc::new(wake));
        let mut context = Context::from_waker(&waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(()) => {
                self.tasks.borrow_mut().remove(id);
            }
            Poll::Pending => {
                let mut tasks = self.tasks.borrow_mut();
                if let Some(task) = tasks.get_mut(id) {
                    task.future = Some(future);
                }
            }
        }
        true
    }

    fn take_ready(&self, id: TaskId) -> Option<ReadyTask> {
        let mut tasks = self.tasks.borrow_mut();
        let task = tasks.get_mut(id)?;
        Some(ReadyTask {
            future: task.future.take().expect("task already running"),
            wake: task.wake.clone(),
        })
    }

    fn cancel(&self, id: TaskId) {
        let _ = self.tasks.borrow_mut().remove(id);
    }

    fn is_finished(&self, id: TaskId) -> bool {
        !self.tasks.borrow().contains_key(id)
    }
}

impl Drop for TaskExecutor {
    fn drop(&mut self) {
        let tasks = std::mem::replace(self.tasks.get_mut(), SlotMap::with_key());
        drop(tasks);
    }
}

struct Task {
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    wake: TaskWake,
}

struct ReadyTask {
    future: Pin<Box<dyn Future<Output = ()>>>,
    wake: TaskWake,
}

#[derive(Clone)]
struct TaskWake {
    id: TaskId,
    wake: Arc<dyn Fn(TaskId) + Send + Sync>,
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
        (self.wake)(self.id);
    }
}

fn task_finished(executor: NonNull<TaskExecutor>, id: TaskId) -> bool {
    // safety: handles cannot outlive the pinned executor that created them
    unsafe { executor.as_ref() }.is_finished(id)
}

fn cancel_task(executor: NonNull<TaskExecutor>, id: TaskId) {
    // safety: handles cannot outlive the pinned executor that created them
    unsafe { executor.as_ref() }.cancel(id)
}

#[cfg(test)]
mod test {
    use std::{
        collections::VecDeque,
        future::pending,
        sync::{Arc, Mutex},
    };

    use super::*;

    fn executor() -> (TaskExecutor, Arc<Mutex<VecDeque<TaskId>>>) {
        let ready = Arc::new(Mutex::new(VecDeque::new()));
        let executor = TaskExecutor::new({
            let ready = ready.clone();
            move |task| ready.lock().unwrap().push_back(task)
        });
        (executor, ready)
    }

    #[test]
    fn canceled_slots_reject_stale_ids() {
        let (executor, ready) = executor();
        let task = executor.spawn(pending::<()>());
        let stale = ready.lock().unwrap().pop_front().unwrap();
        drop(task);
        assert!(!executor.run(stale));
        assert!(ready.lock().unwrap().is_empty());

        let task = executor.spawn(async {});
        let current = ready.lock().unwrap().pop_front().unwrap();
        assert_ne!(stale, current);
        assert!(!executor.run(stale));
        assert!(executor.run(current));
        assert!(task.is_finished());
    }
}
