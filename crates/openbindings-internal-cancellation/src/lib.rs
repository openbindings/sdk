#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
use openbindings::{Cancelled, WorkControl};
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

/// Executor-independent cooperative cancellation. Clones share permanent state.
/// Use a fresh token for each independent retry. Cancellation wakes every registered
/// waiter synchronously, outside the registry lock; it does not schedule a task.
/// Import this type through the async companion whose role you use.
///
/// ```
/// use openbindings_internal_cancellation::CancellationToken;
/// let attempt = CancellationToken::new();
/// attempt.clone().cancel();
/// assert!(attempt.check().is_err());
/// assert!(!CancellationToken::new().is_cancelled());
/// ```
#[derive(Clone, Default)]
pub struct CancellationToken {
    state: Arc<State>,
}
impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("is_cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct State {
    cancelled: AtomicBool,
    next_id: AtomicUsize,
    waiters: Mutex<HashMap<usize, Waker>>,
}
impl CancellationToken {
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
    /// Check the shared state at a cooperative work boundary.
    ///
    /// # Errors
    /// Returns `Cancelled` once the caller has cancelled this work attempt.
    pub fn check(&self) -> Result<(), Cancelled> {
        WorkControl::check(self)
    }
    /// Resolves when cancelled. Dropping this future unregisters its waker.
    pub fn cancelled(&self) -> Cancellation {
        Cancellation {
            control: self.clone(),
            id: None,
        }
    }
}
impl WorkControl for CancellationToken {
    fn is_cancelled(&self) -> bool {
        self.is_cancelled()
    }
}
#[must_use = "cancellation futures must be polled or awaited"]
/// Executor-independent future that resolves on [`CancellationToken::cancel`]. Polling registers a scoped waker; dropping the future unregisters it. It owns a clone of the cancellation state, not a task or timer.
pub struct Cancellation {
    control: CancellationToken,
    id: Option<usize>,
}
impl std::fmt::Debug for Cancellation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cancellation")
            .field("control", &self.control)
            .finish_non_exhaustive()
    }
}

impl Future for Cancellation {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.get_mut().poll_with_clone(cx, Waker::clone)
    }
}
impl Cancellation {
    // The clone callback lets tests cancel deterministically in both registration
    // windows without unsafe RawWakers or production observation counters.
    fn poll_with_clone(
        &mut self,
        cx: &mut Context<'_>,
        clone_waker: impl FnOnce(&Waker) -> Waker,
    ) -> Poll<()> {
        let this = self;
        let id = {
            let waiters = this.control.state.waiters.lock().unwrap();
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
                .is_some_and(|old| old.will_wake(cx.waker()))
            {
                return Poll::Pending;
            }
            id
        };
        // RawWaker clone and last-reference destruction may reenter cancellation.
        let waker = clone_waker(cx.waker());
        let old = {
            let mut waiters = this.control.state.waiters.lock().unwrap();
            // Cancellation may have run during clone. Register only while live,
            // under the same lock used by cancel, so no wakeup can be lost.
            if this.control.is_cancelled() {
                drop(waiters);
                drop(waker);
                return Poll::Ready(());
            }
            waiters.insert(id, waker)
        };
        drop(old);
        if this.control.is_cancelled() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
impl Drop for Cancellation {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            let removed = self.control.state.waiters.lock().unwrap().remove(&id);
            drop(removed);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, task::Wake, time::Duration};
    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[test]
    fn waiters_wake_and_are_released_on_cancel_or_drop() {
        let control = CancellationToken::new();
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

    struct CancelOnDrop(CancellationToken);
    // The owned, reentrant destructor is the witness; Waker::noop cannot supply it.
    #[allow(clippy::manual_noop_waker)]
    impl Wake for CancelOnDrop {
        fn wake(self: Arc<Self>) {}
    }
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }

    #[test]
    fn last_waker_drop_and_replacement_can_reenter_cancellation() {
        for replace in [false, true] {
            let (done, result) = mpsc::channel();
            let task = std::thread::spawn(move || {
                let control = CancellationToken::new();
                let mut wait = control.cancelled();
                let waker = Waker::from(Arc::new(CancelOnDrop(control.clone())));
                assert!(
                    Pin::new(&mut wait)
                        .poll(&mut Context::from_waker(&waker))
                        .is_pending()
                );
                drop(waker); // The registered waiter now owns the last reference.
                if replace {
                    let _ = Pin::new(&mut wait).poll(&mut Context::from_waker(Waker::noop()));
                    assert!(
                        Pin::new(&mut wait)
                            .poll(&mut Context::from_waker(Waker::noop()))
                            .is_ready()
                    );
                }
                drop(wait);
                assert!(control.is_cancelled());
                assert!(control.state.waiters.lock().unwrap().is_empty());
                done.send(()).unwrap();
            });
            result
                .recv_timeout(Duration::from_secs(5))
                .expect("waker destruction must not deadlock cancellation");
            task.join().unwrap();
        }
    }

    #[test]
    fn cancellation_racing_registration_never_loses_a_wakeup() {
        for _ in 0..256 {
            let control = CancellationToken::new();
            let cancel = control.clone();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let release = barrier.clone();
            let task = std::thread::spawn(move || {
                release.wait();
                cancel.cancel();
            });
            let counter = Arc::new(Counter::default());
            let waker = Waker::from(counter.clone());
            let mut cx = Context::from_waker(&waker);
            let mut wait = control.cancelled();
            barrier.wait();
            let result = Pin::new(&mut wait).poll(&mut cx);
            task.join().unwrap();
            if result.is_pending() {
                assert_eq!(counter.0.load(Ordering::Relaxed), 1);
            }
            assert!(Pin::new(&mut wait).poll(&mut cx).is_ready());
            assert!(control.state.waiters.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn cancellation_before_poll_and_in_both_clone_windows_is_ready() {
        for phase in 0..3 {
            let token = CancellationToken::new();
            let owner = Arc::new(Counter::default());
            let weak = Arc::downgrade(&owner);
            let waker = Waker::from(owner);
            let mut wait = token.cancelled();
            if phase == 0 {
                token.cancel();
            }
            assert!(
                wait.poll_with_clone(&mut Context::from_waker(&waker), |waker| {
                    if phase == 1 {
                        token.cancel();
                    }
                    let cloned = waker.clone();
                    if phase == 2 {
                        token.cancel();
                    }
                    cloned
                })
                .is_ready()
            );
            assert!(token.state.waiters.lock().unwrap().is_empty());
            drop(wait);
            drop(waker);
            assert!(weak.upgrade().is_none());
        }
    }

    #[test]
    fn distinct_waiters_replace_only_their_own_registration_and_cancel_once() {
        let token = CancellationToken::new();
        let a = Arc::new(Counter::default());
        let b = Arc::new(Counter::default());
        let replacement = Arc::new(Counter::default());
        let wa = Waker::from(a.clone());
        let wb = Waker::from(b.clone());
        let wr = Waker::from(replacement.clone());
        let mut first = token.cancelled();
        let mut second = token.cancelled();
        for (wait, waker) in [(&mut first, &wa), (&mut second, &wb)] {
            assert!(
                Pin::new(wait)
                    .poll(&mut Context::from_waker(waker))
                    .is_pending()
            );
        }
        assert!(
            Pin::new(&mut first)
                .poll(&mut Context::from_waker(&wr))
                .is_pending()
        );
        token.cancel();
        token.cancel();
        assert_eq!(a.0.load(Ordering::Relaxed), 0);
        assert_eq!(b.0.load(Ordering::Relaxed), 1);
        assert_eq!(replacement.0.load(Ordering::Relaxed), 1);
        assert!(token.state.waiters.lock().unwrap().is_empty());
    }

    #[test]
    fn unpolled_pending_and_completed_waiters_release_owners() {
        for phase in 0..3 {
            let token = CancellationToken::new();
            let weak_state = Arc::downgrade(&token.state);
            let owner = Arc::new(Counter::default());
            let weak_waker = Arc::downgrade(&owner);
            let waker = Waker::from(owner);
            let mut wait = token.cancelled();
            if phase > 0 {
                assert!(
                    Pin::new(&mut wait)
                        .poll(&mut Context::from_waker(&waker))
                        .is_pending()
                );
            }
            if phase == 2 {
                token.cancel();
                assert!(
                    Pin::new(&mut wait)
                        .poll(&mut Context::from_waker(&waker))
                        .is_ready()
                );
            }
            drop(waker);
            drop(wait);
            assert!(weak_waker.upgrade().is_none());
            drop(token);
            assert!(weak_state.upgrade().is_none());
        }
    }

    struct ReentrantWake(CancellationToken);
    impl Wake for ReentrantWake {
        fn wake(self: Arc<Self>) {
            self.0.cancel();
            let mut waiter = self.0.cancelled();
            assert!(
                Pin::new(&mut waiter)
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_ready()
            );
        }
    }
    #[test]
    fn waking_can_reenter_cancellation_and_poll() {
        let (done, result) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let token = CancellationToken::new();
            let weak = Arc::downgrade(&token.state);
            let waker = Waker::from(Arc::new(ReentrantWake(token.clone())));
            let mut wait = token.cancelled();
            assert!(
                Pin::new(&mut wait)
                    .poll(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            token.cancel();
            drop((waker, wait, token));
            assert!(weak.upgrade().is_none());
            done.send(()).unwrap();
        });
        result
            .recv_timeout(Duration::from_secs(5))
            .expect("reentrant wake must not deadlock");
        thread.join().unwrap();
    }
}
