//! One nongeneric-provider channel's validation handshake. Callers hold the
//! lifecycle lock while changing this state, but move returned values out before
//! dropping them or invoking evaluators. Acknowledgement never commits output.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RequestId(u64);

enum State<T> {
    Idle,
    Pending(RequestId, T),
    Checking(RequestId),
    Accepted(RequestId, T),
    Closed,
}

pub(super) enum OfferFailure<T> {
    Busy(T),
    Closed(T),
    Exhausted(T),
}

pub(super) struct Slot<T> {
    state: State<T>,
    next: Option<u64>,
}
impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            state: State::Idle,
            next: Some(0),
        }
    }
}
impl<T> Slot<T> {
    /// Failure returns the offered owner; nothing is dropped under the lock.
    pub(super) fn offer(&mut self, value: T) -> Result<RequestId, OfferFailure<T>> {
        match &self.state {
            State::Idle => {}
            State::Closed => return Err(OfferFailure::Closed(value)),
            _ => return Err(OfferFailure::Busy(value)),
        }
        let Some(next) = self.next else {
            return Err(OfferFailure::Exhausted(value));
        };
        self.next = next.checked_add(1);
        let id = RequestId(next);
        self.state = State::Pending(id, value);
        Ok(id)
    }
    /// The driver checks this value outside the lifecycle lock. Checking keeps
    /// its identity active until completion, withdrawal or owner closure.
    pub(super) fn begin_check(&mut self) -> Option<(RequestId, T)> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Pending(id, value) => {
                self.state = State::Checking(id);
                Some((id, value))
            }
            other => {
                self.state = other;
                None
            }
        }
    }
    /// Every result, including a failure, must check this identity under the
    /// same lock as terminal arbitration. Cancellation is independently global.
    pub(super) fn checking(&self, id: RequestId) -> bool {
        matches!(self.state, State::Checking(current) if current == id)
    }
    pub(super) fn accepted(&self, id: RequestId) -> bool {
        matches!(self.state, State::Accepted(current, _) if current == id)
    }
    /// Acknowledgement only makes a value available to its live provider owner.
    /// A stale result returns its owner for disposal after unlocking.
    pub(super) fn accept(&mut self, id: RequestId, value: T) -> Result<(), T> {
        if !self.checking(id) {
            return Err(value);
        }
        self.state = State::Accepted(id, value);
        Ok(())
    }
    /// Output takes this only in its own live emission poll, together with queue
    /// capacity and terminal checks, and returns Ready in that same operation.
    pub(super) fn take_accepted(&mut self, id: RequestId) -> Option<T> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Accepted(current, value) if current == id => Some(value),
            other => {
                self.state = other;
                None
            }
        }
    }
    /// A dropped output offer invalidates both successful and failed late checks.
    /// Input uses this when its owner declines delivery, not on a cancelled read.
    pub(super) fn withdraw(&mut self, id: RequestId) -> Option<T> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Pending(current, value) | State::Accepted(current, value) if current == id => {
                Some(value)
            }
            State::Checking(current) if current == id => None,
            other => {
                self.state = other;
                None
            }
        }
    }
    /// Terminal or provider half-close revokes every outstanding request. Return
    /// retained values so their destructors run after releasing the lock.
    pub(super) fn close(&mut self) -> Option<T> {
        match std::mem::replace(&mut self.state, State::Closed) {
            State::Pending(_, value) | State::Accepted(_, value) => Some(value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    fn offer<T>(slot: &mut Slot<T>, value: T) -> RequestId {
        match slot.offer(value) {
            Ok(id) => id,
            Err(_) => panic!("idle slot accepts a request"),
        }
    }

    #[test]
    fn acknowledgement_is_not_output_commit_and_withdrawal_wins_before_commit() {
        for withdraw_at in 0..3 {
            let mut slot = Slot::default();
            let id = offer(&mut slot, "original");
            if withdraw_at == 0 {
                assert_eq!(slot.withdraw(id), Some("original"));
                assert!(slot.begin_check().is_none());
            } else {
                let (checking, value) = slot.begin_check().unwrap();
                assert_eq!(checking, id);
                if withdraw_at == 1 {
                    assert_eq!(slot.withdraw(id), None);
                    assert_eq!(slot.accept(id, value), Err("original"));
                } else {
                    assert_eq!(slot.accept(id, value), Ok(()));
                    assert_eq!(slot.withdraw(id), Some("original"));
                }
            }
            assert_eq!(slot.take_accepted(id), None);
        }
        let mut slot = Slot::default();
        let id = offer(&mut slot, "original");
        let (_, value) = slot.begin_check().unwrap();
        slot.accept(id, value).unwrap();
        assert_eq!(slot.take_accepted(id), Some("original"));
        assert_eq!(slot.take_accepted(id), None);
    }

    #[test]
    fn stale_success_and_failure_cannot_acknowledge_or_reject_the_next_request() {
        let mut slot = Slot::default();
        let old = offer(&mut slot, "A");
        let (_, value) = slot.begin_check().unwrap();
        slot.withdraw(old);
        let next = offer(&mut slot, "B");
        assert_ne!(old, next);
        assert!(!slot.checking(old)); // Guard for mismatches and preparation failures too.
        assert_eq!(slot.accept(old, value), Err("A"));
        assert_eq!(slot.withdraw(old), None);
        assert_eq!(slot.begin_check(), Some((next, "B")));
        assert!(slot.checking(next));
        assert!(!slot.checking(old));
        assert_eq!(slot.accept(old, "late A"), Err("late A"));
        slot.accept(next, "B").unwrap();
        assert_eq!(slot.take_accepted(old), None);
        assert_eq!(slot.take_accepted(next), Some("B"));
    }

    #[test]
    fn half_close_or_terminal_revokes_every_phase() {
        for phase in 0..3 {
            let mut slot = Slot::default();
            let id = offer(&mut slot, 7);
            let mut outside = None;
            if phase > 0 {
                outside = slot.begin_check().map(|(_, value)| value);
                if phase == 2 {
                    slot.accept(id, outside.take().unwrap()).unwrap();
                }
            }
            assert_eq!(slot.close(), if phase == 1 { None } else { Some(7) });
            assert!(!slot.checking(id));
            if let Some(value) = outside {
                assert_eq!(slot.accept(id, value), Err(7));
            }
            assert_eq!(slot.take_accepted(id), None);
            assert_eq!(slot.withdraw(id), None);
            assert!(matches!(slot.offer(8), Err(OfferFailure::Closed(8))));
        }
    }

    #[test]
    fn retained_values_leave_the_slot_before_their_destructors_run() {
        struct Owner(Rc<Cell<usize>>);
        impl Drop for Owner {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        for phase in 0..3 {
            let drops = Rc::new(Cell::new(0));
            let mut slot = Slot::default();
            let id = offer(&mut slot, Owner(drops.clone()));
            let outside = if phase == 0 {
                slot.close().unwrap()
            } else {
                let (_, value) = slot.begin_check().unwrap();
                if phase == 1 {
                    slot.withdraw(id);
                    match slot.accept(id, value) {
                        Err(value) => value,
                        Ok(()) => panic!("withdrawn request"),
                    }
                } else {
                    assert!(slot.accept(id, value).is_ok());
                    slot.take_accepted(id).unwrap()
                }
            };
            assert_eq!(drops.get(), 0);
            drop(outside);
            assert_eq!(drops.get(), 1);
        }
    }

    #[test]
    fn a_busy_slot_preserves_its_owner_and_identity_never_wraps() {
        let mut slot = Slot {
            state: State::Idle,
            next: Some(u64::MAX),
        };
        let id = offer(&mut slot, 1);
        assert!(matches!(slot.offer(2), Err(OfferFailure::Busy(2))));
        assert_eq!(slot.withdraw(id), Some(1));
        assert!(matches!(slot.offer(3), Err(OfferFailure::Exhausted(3))));
        assert!(!slot.checking(id));
        assert!(slot.begin_check().is_none());
    }
}
