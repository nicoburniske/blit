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
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    task::{Context, Poll},
};

use async_task::Runnable;
use futures_lite::{Stream, StreamExt};

#[must_use = "dropping this handle cancels the task"]
/// a local task that is cancelled when dropped
pub struct TaskHandle<T = ()> {
    task: async_task::Task<T>,
    marker: PhantomData<Rc<()>>,
}

impl<T> TaskHandle<T> {
    pub fn detach(self) {
        self.task.detach()
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl<T> Future for TaskHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.task).poll(context)
    }
}

/// operations available to futures running on the application thread
pub struct Ops<A: 'static> {
    executor: NonNull<LocalExecutor<A>>,
    local: PhantomData<Rc<()>>,
}

impl<A> Copy for Ops<A> {}

impl<A> Clone for Ops<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: 'static> Ops<A> {
    /// schedules a `!Send` future on the local application thread
    pub fn spawn<F>(&self, future: F) -> TaskHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let sender = self.executor().sender.clone();
        let wake = self.executor().wake.clone();
        let (runnable, task) = async_task::spawn_local(future, move |runnable| {
            match sender.send(runnable) {
                Ok(()) => wake(),
                Err(error) => {
                    // a local runnable cannot be dropped by a foreign waker after shutdown
                    std::mem::forget(error.0);
                }
            }
        });
        runnable.schedule();
        TaskHandle {
            task,
            marker: PhantomData,
        }
    }

    #[track_caller]
    /// provides mutable application access for the remainder of the current task poll
    ///
    /// the returned handle must be dropped before awaiting another future
    pub fn app(&self) -> AppMut<A> {
        let executor = self.executor();
        let app = match executor.app.get() {
            AppAccess::Available(app) => app,
            AppAccess::Borrowed(location) => panic!("application already borrowed at {location}"),
            AppAccess::Unavailable => panic!("application access outside task poll"),
        };
        executor.app.set(AppAccess::Borrowed(Location::caller()));
        AppMut { ops: *self, app }
    }

    /// runs a future and handles its output with mutable application access
    pub fn handle<F, C>(&self, future: F, complete: C) -> TaskHandle
    where
        F: Future + 'static,
        F::Output: 'static,
        C: FnOnce(&mut A, F::Output) + 'static,
    {
        let ops = *self;
        self.spawn(async move {
            let output = future.await;
            complete(&mut ops.app(), output);
        })
    }

    /// runs a stream and handles each item with mutable application access
    pub fn handle_stream<S, C>(&self, stream: S, mut receive: C) -> TaskHandle
    where
        S: Stream + 'static,
        C: FnMut(&mut A, S::Item) + 'static,
    {
        let ops = *self;
        self.spawn(async move {
            futures_lite::pin!(stream);
            while let Some(item) = stream.next().await {
                receive(&mut ops.app(), item);
            }
        })
    }

    fn executor(&self) -> &LocalExecutor<A> {
        // safety: the platform keeps the pinned executor alive while Ops can be used
        unsafe { self.executor.as_ref() }
    }
}

#[must_not_suspend = "mutable app access cannot be held across a suspend point"]
/// exclusive mutable application access during a task poll
pub struct AppMut<A: 'static> {
    ops: Ops<A>,
    app: NonNull<A>,
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
        self.ops.executor().app.set(AppAccess::Available(self.app));
    }
}

/// a local task queue driven by a platform event loop
pub struct LocalExecutor<A: 'static> {
    sender: Sender<Runnable>,
    receiver: Receiver<Runnable>,
    wake: Arc<dyn Fn() + Send + Sync>,
    app: Cell<AppAccess<A>>,
    _pinned: PhantomPinned,
}

impl<A: 'static> LocalExecutor<A> {
    /// creates an executor that calls `wake` whenever local work becomes runnable
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        Self {
            sender,
            receiver,
            wake,
            app: Cell::new(AppAccess::Unavailable),
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
            executor: NonNull::from(self.get_ref()),
            local: PhantomData,
        }
    }

    /// polls up to 64 runnable tasks with temporary access to `app`
    pub fn run(self: Pin<&Self>, app: &mut A) -> bool {
        struct ResetAppSlot<'a, A>(&'a Cell<AppAccess<A>>);

        impl<A> Drop for ResetAppSlot<'_, A> {
            fn drop(&mut self) {
                self.0.set(AppAccess::Unavailable);
            }
        }

        let executor = self.get_ref();
        assert!(
            matches!(executor.app.get(), AppAccess::Unavailable),
            "tasks already running"
        );
        executor.app.set(AppAccess::Available(NonNull::from(app)));
        let _reset_app_slot = ResetAppSlot(&executor.app);
        let mut ran = false;

        for _ in 0..64 {
            match executor.receiver.try_recv() {
                Ok(runnable) => {
                    ran = true;
                    runnable.run();
                    match executor.app.get() {
                        AppAccess::Available(_) => {}
                        AppAccess::Borrowed(location) => {
                            panic!(
                                "app borrowed at {location} across await. add these attributes to your crate:\n\
                                 #![feature(must_not_suspend)]\n\
                                 #![deny(must_not_suspend)]"
                            )
                        }
                        AppAccess::Unavailable => panic!("app access state lost during task poll"),
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return ran,
            }
        }

        (executor.wake)();
        ran
    }
}

enum AppAccess<A> {
    Unavailable,
    Available(NonNull<A>),
    Borrowed(&'static Location<'static>),
}

impl<A> Copy for AppAccess<A> {}

impl<A> Clone for AppAccess<A> {
    fn clone(&self) -> Self {
        *self
    }
}
