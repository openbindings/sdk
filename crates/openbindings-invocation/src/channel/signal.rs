//! Wake registrations shared by independently polled channel halves.
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Wake, Waker},
};
#[derive(Default)]
pub(super) struct Signal {
    next: AtomicUsize,
    generation: AtomicUsize,
    waiters: Mutex<HashMap<usize, Arc<Waker>>>,
}
impl Signal {
    pub(super) fn notify(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let waiters: Vec<_> = self.waiters.lock().unwrap().values().cloned().collect();
        for w in waiters {
            w.wake_by_ref();
        }
    }
}
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.notify();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}
pub(super) struct Waiter {
    signal: Arc<Signal>,
    id: Option<usize>,
}
impl Waiter {
    pub(super) fn new(signal: Arc<Signal>) -> Self {
        Self { signal, id: None }
    }
    pub(super) fn clear(&mut self) {
        if let Some(id) = self.id.take() {
            let removed = self.signal.waiters.lock().unwrap().remove(&id);
            drop(removed); // A Waker destructor can reenter just like wake().
        }
    }
    pub(super) fn register(&mut self, waker: &Waker) {
        let id = {
            let waiters = self.signal.waiters.lock().unwrap();
            let id = *self.id.get_or_insert_with(|| {
                loop {
                    let id = self.signal.next.fetch_add(1, Ordering::Relaxed);
                    if !waiters.contains_key(&id) {
                        break id;
                    }
                }
            });
            if waiters.get(&id).is_some_and(|old| old.will_wake(waker)) {
                return;
            }
            id
        };
        let new = Arc::new(waker.clone()); // RawWaker clone may also reenter.
        let old = self.signal.waiters.lock().unwrap().insert(id, new);
        drop(old);
    }
}
impl Drop for Waiter {
    fn drop(&mut self) {
        self.clear();
    }
}
