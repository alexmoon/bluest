use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::task;
use std::time::Duration;

use async_broadcast::{Receiver, Sender};
use async_lock::{Mutex, MutexGuard};
use futures_core::Stream;
use futures_lite::{FutureExt, StreamExt};
use futures_timer::Delay;

/// Reusable exclusive register for `ExcluderLock`.
pub struct Excluder<T: Send + Clone> {
    inner: Mutex<Weak<Sender<()>>>,
    last_val: Arc<Mutex<Option<T>>>,
}

/// Prevents other tasks from doing the same operation before the corresponding
/// "foreign" callback is reiceived by the current task. Unlocks on dropping.
pub struct ExcluderLock<T: Send + Clone> {
    #[allow(unused)]
    inner: Option<Arc<Sender<()>>>, // always `Some` before `drop()`
    receiver: Receiver<()>,
    last_val: Weak<Mutex<Option<T>>>,
}

impl<T: Send + Clone, E: Send + Clone> Excluder<Result<T, E>> {
    /// Locks the excluder, does the operation that will produce the callback,
    /// then waits for the callback's result.
    #[allow(unused)]
    pub async fn obtain(&self, operation: impl FnOnce() -> Result<(), E>) -> Result<Option<T>, E> {
        let lock = self.lock().await;
        operation()?;
        if let Some(res) = lock.wait_unlock().await {
            Ok(Some(res?))
        } else {
            Ok(None)
        }
    }
}

impl<T: Send + Clone> Excluder<T> {
    /// Creates a new unlocked `Excluder`.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Weak::new()),
            last_val: Arc::new(Mutex::new(None)),
        }
    }

    /// Clones and returns the last value returned by the "foreign" callback.
    pub fn last_value(&self) -> Option<T> {
        self.last_val.lock_blocking().clone()
    }

    /// Checks if the excluder is locked.
    #[allow(unused)]
    pub fn is_locked(&self) -> bool {
        // Don't call it in this module
        self.inner.lock_blocking().strong_count() > 0
    }

    /// Waits until the excluder is unlocked and locks the excluder.
    /// Call this right before calling a method that will produce a "foreign" callback;
    /// after calling that method, call [ExcluderLock::wait_unlock] in the same task.
    pub async fn lock(&self) -> ExcluderLock<T> {
        // waits for the waking signal if the excluder is currently locked.
        let receiver = {
            let guard_inner = self.inner.lock().await;
            guard_inner.upgrade().as_ref().map(|s| s.new_receiver())
        };
        if let Some(mut receiver) = receiver {
            // to prevent dead lock, don't hold the `Arc<Sender<()>>` during waiting.
            let _ = receiver.recv().await;
        }

        let mut guard_inner = self.inner.lock().await;
        if guard_inner.strong_count() > 0 {
            // race condition of multiple tasks trying to lock after receiving unlock signal;
            // one of them has already won, just wait for that new lock to be unlocked.
            drop(guard_inner);
            return Box::pin(self.lock()).await;
        }
        // don't drop the guard before setting the lock; `async_lock` is used for this requirement.
        self.unchecked_set_lock(&mut guard_inner)
    }

    /// Locks the excluder if it is previously unlocked.
    pub fn try_lock(&self) -> Option<ExcluderLock<T>> {
        let mut guard_inner = self.inner.lock_blocking();
        if guard_inner.strong_count() == 0 {
            Some(self.unchecked_set_lock(&mut guard_inner))
        } else {
            None
        }
    }

    // Please ensure `guard_inner.strong_count() == 0` before calling this.
    fn unchecked_set_lock(&self, guard_inner: &mut MutexGuard<Weak<Sender<()>>>) -> ExcluderLock<T> {
        let (sender, receiver) = async_broadcast::broadcast(1);
        let sender = Arc::new(sender);
        **guard_inner = Arc::downgrade(&sender); // sets the lock
        ExcluderLock {
            inner: Some(sender),
            receiver,
            last_val: Arc::downgrade(&self.last_val),
        }
    }

    /// Sends the "completed" (unlock) signal from the "foreign" callback.
    pub fn unlock(&self, result: T) {
        self.last_val.lock_blocking().replace(result);

        let mut guard_inner = self.inner.lock_blocking();
        if let Some(sender) = guard_inner.upgrade() {
            // to prevent dead lock, invalidate the `Weak` in `Excluder` before broadcasting.
            *guard_inner = Weak::new();
            let _ = sender.broadcast_blocking(());
        }
    }
}

impl<T: Send + Clone> Default for Excluder<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Clone> Drop for Excluder<T> {
    fn drop(&mut self) {
        // makes sure `ExcluderLock::wait_unlock` return `None`.
        let _ = self.last_val.lock_blocking().take();

        let mut guard_inner = self.inner.lock_blocking();
        if let Some(sender) = guard_inner.upgrade() {
            *guard_inner = Weak::new();
            let _ = sender.broadcast_blocking(());
        }
    }
}

// XXX: have global timeout values in `AdapterConfig` and add a timeout argument here.
impl<T: Send + Clone> ExcluderLock<T> {
    /// Waits until the unlock signal is sent from the "foreign" callback.
    /// Returns `None` when the corresponding `Excluder` is dropped.
    pub async fn wait_unlock(mut self) -> Option<T> {
        self.receiver.recv().await.ok()?;
        self.last_val
            .upgrade()
            .and_then(|arc| arc.lock_blocking().as_ref().cloned())
    }

    /// Waits until the unlock signal is sent from the "foreign" callback or the timeout
    /// is reached. Returns `None` when timeout or when the corresponding `Excluder` is dropped.
    pub async fn wait_unlock_with_timeout(self, timeout: Duration) -> Option<T> {
        self.wait_unlock()
            .or(async {
                Delay::new(timeout).await;
                None
            })
            .await
    }
}

/// Sends notifications from "foreign" callbacks if there is any existing `NotifierReceiver`.
pub struct Notifier<T: Send + Clone> {
    capacity: usize,
    inner: Mutex<Weak<NotifierInner<T>>>,
}

struct NotifierInner<T: Send + Clone> {
    sender: Sender<Option<T>>,
    on_stop: Box<dyn Fn() + Send + Sync + 'static>,
}

pub struct NotifierReceiver<T: Send + Clone> {
    holder: Option<Arc<NotifierInner<T>>>,
    receiver: Receiver<Option<T>>,
}

impl<T: Send + Clone> Notifier<T> {
    /// Creates a new inactive `Notifier`.
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(Weak::new()),
        }
    }

    /// Checks if the notifier is active.
    pub fn is_notifying(&self) -> bool {
        // Don't call it in this module
        self.inner.lock_blocking().strong_count() > 0
    }

    /// Creates a new `NotifierReceiver` for the caller to receive notifications.
    /// - `on_start` is called while locking the notifier if the notifier is not active.
    /// - `on_stop` is what the notifier should do when it is deactivated, but it is not
    ///   replaced if the notifier is already active.
    pub async fn subscribe<E>(
        &self,
        on_start: impl FnOnce() -> Result<(), E>,
        on_stop: impl Fn() + Send + Sync + 'static,
    ) -> Result<NotifierReceiver<T>, E> {
        let mut guard_inner = self.inner.lock().await;
        if let Some(inner) = guard_inner.upgrade() {
            let receiver = inner.sender.new_receiver();
            Ok(NotifierReceiver {
                holder: Some(inner),
                receiver,
            })
        } else {
            on_start()?;
            let (mut sender, receiver) = async_broadcast::broadcast(self.capacity);
            sender.set_overflow(true);
            let new_inner = Arc::new(NotifierInner {
                sender,
                on_stop: Box::new(on_stop),
            });
            *guard_inner = Arc::downgrade(&new_inner);
            Ok(NotifierReceiver {
                holder: Some(new_inner),
                receiver,
            })
        }
    }

    /// Sends a notifcation value from the "foreign" callback.
    pub fn notify(&self, value: T) {
        let inner = self.inner.lock_blocking().upgrade();
        if let Some(inner) = inner {
            let _ = inner.sender.broadcast_blocking(Some(value));
        }
    }
}

impl<T: Send + Clone> futures_core::Stream for NotifierReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Option<T>> {
        if self.holder.is_none() {
            task::Poll::Ready(None)
        } else if let task::Poll::Ready(result) = std::pin::pin!(&mut self.receiver).poll_next(cx) {
            if let Some(value) = result.flatten() {
                task::Poll::Ready(Some(value))
            } else {
                let _ = self.holder.take();
                task::Poll::Ready(None)
            }
        } else {
            task::Poll::Pending
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.receiver.size_hint()
    }
}

impl<T: Send + Clone> Drop for Notifier<T> {
    fn drop(&mut self) {
        let inner = self.inner.lock_blocking().upgrade();
        if let Some(inner) = inner {
            let _ = inner.sender.broadcast_blocking(None);
        }
    }
}

impl<T: Send + Clone> Drop for NotifierInner<T> {
    fn drop(&mut self) {
        (self.on_stop)()
    }
}

/// Wraps the main stream and also checks an event stream; ends and fuses the main stream when
/// the event stream ends or the checker returns true for a received event item.
pub struct StreamUntil<T, E, S, F>
where
    T: Send + Unpin,
    E: Send,
    S: Stream<Item = E> + Send + Unpin,
    F: Fn(&E) -> bool + Send + Sync + Unpin + 'static,
{
    stream: S,
    event_checker: F,
    ph: PhantomData<T>,
}

impl<T, E, S, F> StreamUntil<T, E, S, F>
where
    T: Send + Unpin,
    E: Send,
    S: Stream<Item = E> + Send + Unpin,
    F: Fn(&E) -> bool + Send + Sync + Unpin + 'static,
{
    /// Creates the `StreamUntil`.
    pub fn create(stream: impl Stream<Item = T>, event_stream: S, event_checker: F) -> impl Stream<Item = T> {
        stream
            .or(StreamUntil {
                stream: event_stream,
                event_checker,
                ph: PhantomData,
            })
            .fuse()
    }
}

impl<T, E, S, F> futures_core::Stream for StreamUntil<T, E, S, F>
where
    T: Send + Unpin,
    E: Send,
    S: Stream<Item = E> + Send + Unpin,
    F: Fn(&E) -> bool + Send + Sync + Unpin + 'static,
{
    type Item = T;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use futures_core::task::Poll;
        match self.stream.poll_next(cx) {
            Poll::Ready(Some(event)) if (self.event_checker)(&event) => Poll::Ready(None),
            Poll::Ready(None) => Poll::Ready(None),
            _ => Poll::Pending,
        }
    }
}
