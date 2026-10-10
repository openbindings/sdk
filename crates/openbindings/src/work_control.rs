//! Executor-independent cancellation with scoped waiter ownership.
use crate::{NoVerdict, NoVerdictReason};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};

/// Cooperative cancellation shared by synchronous work and asynchronous I/O.
/// Clones share one cancellation state. Cancellation is permanent for that state;
/// use a fresh control for an independent retry. Work checks it at cooperative
/// boundaries, so this is not a preemptive deadline. Application code owns scheduling.
/// Synchronous Wasm cannot observe a same-thread AbortSignal until it yields;
/// a worker is required for externally interruptible synchronous work.
///
/// ```
/// use openbindings::{NoVerdictReason, WorkControl};
/// let attempt = WorkControl::new();
/// let cancellation = attempt.clone();
/// cancellation.cancel();
/// assert_eq!(attempt.check().unwrap_err().reason, NoVerdictReason::Cancelled);
/// let retry = WorkControl::new();
/// assert!(retry.check().is_ok());
/// ```
#[derive(Clone, Default)]
pub struct WorkControl {
    state: Arc<State>,
}
#[derive(Default)]
struct State {
    cancelled: AtomicBool,
    next_id: AtomicUsize,
    waiters: Mutex<HashMap<usize, Waker>>,
}
impl WorkControl {
    /// Create a fresh, uncancelled state.
    pub fn new() -> Self {
        Self::default()
    }
    /// Permanently cancel this state and its clones, waking registered waiters.
    pub fn cancel(&self) {
        let waiters = {
            let mut waiters = self.state.waiters.lock().unwrap();
            self.state.cancelled.store(true, Ordering::Release);
            std::mem::take(&mut *waiters)
        };
        // A waker may poll or drop its future immediately. Never call it under the lock.
        for (_, waker) in waiters {
            waker.wake();
        }
    }
    /// Read the shared permanent cancellation flag without registering a waiter.
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }
    /// Return a `caller-cancelled` refusal when cancelled; otherwise continue. Call at cooperative work boundaries.
    pub fn check(&self) -> Result<(), NoVerdict> {
        if self.is_cancelled() {
            Err(NoVerdict::new(
                NoVerdictReason::Cancelled,
                "caller-cancelled",
                "the caller cancelled this operation",
            ))
        } else {
            Ok(())
        }
    }
    /// Resolves when cancelled. Dropping this future unregisters its waker.
    pub fn cancelled(&self) -> Cancellation {
        Cancellation {
            control: self.clone(),
            id: None,
        }
    }
}
#[must_use = "cancellation futures must be polled or awaited"]
/// Executor-independent future that resolves on [`WorkControl::cancel`]. Polling registers a scoped waker; dropping the future unregisters it. It owns a clone of the cancellation state, not a task or timer.
pub struct Cancellation {
    control: WorkControl,
    id: Option<usize>,
}
impl Future for Cancellation {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        let mut waiters = this.control.state.waiters.lock().unwrap();
        if this.control.is_cancelled() {
            return Poll::Ready(());
        }
        let id = *this.id.get_or_insert_with(|| {
            loop {
                let candidate = this.control.state.next_id.fetch_add(1, Ordering::Relaxed);
                if !waiters.contains_key(&candidate) {
                    break candidate;
                }
            }
        });
        if waiters
            .get(&id)
            .is_none_or(|old| !old.will_wake(cx.waker()))
        {
            waiters.insert(id, cx.waker().clone());
        }
        Poll::Pending
    }
}
impl Drop for Cancellation {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.control.state.waiters.lock().unwrap().remove(&id);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Wake;
    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[test]
    fn waiters_wake_and_are_released_on_cancel_or_drop() {
        let control = WorkControl::new();
        let counter = Arc::new(Counter::default());
        let waker = Waker::from(counter.clone());
        let mut cx = Context::from_waker(&waker);
        for _ in 0..1000 {
            let mut waiter = control.cancelled();
            assert!(Pin::new(&mut waiter).poll(&mut cx).is_pending());
        }
        assert!(control.state.waiters.lock().unwrap().is_empty());
        let mut a = control.cancelled();
        let mut b = control.cancelled();
        assert!(Pin::new(&mut a).poll(&mut cx).is_pending());
        assert!(Pin::new(&mut b).poll(&mut cx).is_pending());
        control.cancel();
        assert_eq!(counter.0.load(Ordering::Relaxed), 2);
        assert!(Pin::new(&mut a).poll(&mut cx).is_ready());
        assert!(Pin::new(&mut b).poll(&mut cx).is_ready());
        assert!(control.state.waiters.lock().unwrap().is_empty());
    }
}
