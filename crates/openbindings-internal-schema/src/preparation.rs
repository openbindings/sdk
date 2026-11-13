//! One synchronous preparation attempt, never retained by a completed artifact.
//! Cooperative SDK work has one authority: the nested thread-local allowance.
//! Explicit SDK storage reservations are shared through this
//! borrowed control. Neither counter certifies every dependency operation or
//! allocator request; dependency construction has a separate supported profile.
use crate::{Control, Failure, FailureReason};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub const DEFAULT_ALLOCATION_BYTES: usize = 512 * 1024 * 1024;

/// Version-locked bridge to the core's original environment. The companion
/// passes its existing attempt through construction rather than creating a
/// second allowance. This is not another supported evaluator interface.
#[doc(hidden)]
pub trait EnvironmentRequest {
    type Environment: crate::OriginalEnvironment;
    type Failure;
    fn environment_with_control(
        &self,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn Control,
    ) -> Result<Self::Environment, Self::Failure>;
}

pub struct Attempt<'a> {
    control: &'a dyn Control,
    maximum_allocation: usize,
    allocated: AtomicUsize,
    allocation_spent: AtomicBool,
}

impl std::fmt::Debug for Attempt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attempt").finish_non_exhaustive()
    }
}

fn exceeded(resource: &'static str) -> Failure {
    Failure::new(
        FailureReason::LimitExceeded,
        resource,
        "schema preparation exceeded its shared SDK allowance",
    )
}

/// Expanded names are materialized by URI resolution/normalization and retained
/// in several indexes. Their spelling footprint is a separate bound; reserve
/// conservative scratch/copy capacity before starting either expansion pass.
pub(crate) fn identities(bytes: usize, control: &dyn Control) -> Result<(), Failure> {
    control.check()?;
    let Some(allocation) = bytes.checked_mul(64) else {
        control.allocation_limit_reached();
        return Err(exceeded("preparation-sdk-reservation-bytes"));
    };
    let work = bytes
        .checked_mul(8)
        .ok_or_else(|| exceeded("preparation-sdk-work-units"))?;
    control.charge_allocation(allocation)?;
    control.charge_work(work)
}

impl<'a> Attempt<'a> {
    /// Nesting cannot grant more cooperative SDK work than the enclosing attempt has
    /// left. A nested attempt must receive the outer attempt as its control to
    /// retain the aggregate allocation ledger as well.
    pub fn run<T>(
        maximum_work: impl Into<Option<usize>>,
        maximum_allocation: usize,
        control: &'a dyn Control,
        prepare: impl FnOnce(&Self) -> T,
    ) -> Result<T, Failure> {
        control.check()?;
        let attempt = Self {
            control,
            maximum_allocation,
            allocated: AtomicUsize::new(0),
            allocation_spent: AtomicBool::new(false),
        };
        let result = match maximum_work.into() {
            Some(maximum) => openbindings_internal_json::work::scope(maximum, || prepare(&attempt)),
            None => Ok(prepare(&attempt)),
        };
        // Cancellation remains cancellation; an exhausted work/allocation
        // allowance cannot be hidden by a fallback or ignored inner error.
        control.check()?;
        if attempt.allocation_spent.load(Ordering::Relaxed) {
            return Err(exceeded("preparation-sdk-reservation-bytes"));
        }
        result.map_err(|_| exceeded("preparation-sdk-work-units"))
    }

    pub fn allocated(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }
}

impl Control for Attempt<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }
    fn check(&self) -> Result<(), Failure> {
        self.charge_work(1)
    }
    fn charge_work(&self, units: usize) -> Result<(), Failure> {
        // The parent control owns cancellation and any enclosing allocation
        // attempt. Work is charged here once to the authoritative TLS allowance.
        if self.control.is_cancelled() {
            return Err(Failure::new(
                FailureReason::Cancelled,
                "caller-cancelled",
                "the caller cancelled this operation",
            ));
        }
        if self.allocation_is_exhausted() {
            return Err(exceeded("preparation-sdk-reservation-bytes"));
        }
        if !openbindings_internal_json::work::charge(units) {
            return Err(exceeded("preparation-sdk-work-units"));
        }
        Ok(())
    }
    fn charge_allocation(&self, bytes: usize) -> Result<(), Failure> {
        self.check()?;
        // Also debit an explicit enclosing attempt. Default controls only
        // check cancellation; they do not own an allocation allowance.
        self.control.charge_allocation(bytes)?;
        let mut used = self.allocated.load(Ordering::Relaxed);
        loop {
            let Some(total) = used
                .checked_add(bytes)
                .filter(|total| *total <= self.maximum_allocation)
            else {
                self.allocation_limit_reached();
                return Err(exceeded("preparation-sdk-reservation-bytes"));
            };
            match self.allocated.compare_exchange_weak(
                used,
                total,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => used = current,
            }
        }
        Ok(())
    }
    fn allocation_limit_reached(&self) {
        self.allocation_spent.store(true, Ordering::Relaxed);
        self.control.allocation_limit_reached();
    }
    fn release_allocation(&self, bytes: usize) {
        self.allocated.fetch_sub(bytes, Ordering::Relaxed);
        self.control.release_allocation(bytes);
    }
    fn allocation_is_exhausted(&self) -> bool {
        self.allocation_spent.load(Ordering::Relaxed) || self.control.allocation_is_exhausted()
    }
}

/// Borrowed scratch lifetime. Returned artifacts must not retain its storage.
/// Refusals propagate to the parent and remain sticky after scratch is released.
pub struct Scratch<'a> {
    control: &'a dyn Control,
    allocated: AtomicUsize,
}
impl std::fmt::Debug for Scratch<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scratch").finish_non_exhaustive()
    }
}
impl<'a> Scratch<'a> {
    pub fn new(control: &'a dyn Control) -> Self {
        Self {
            control,
            allocated: AtomicUsize::new(0),
        }
    }
}
impl Drop for Scratch<'_> {
    fn drop(&mut self) {
        self.control
            .release_allocation(self.allocated.load(Ordering::Relaxed));
    }
}
impl Control for Scratch<'_> {
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }
    fn check(&self) -> Result<(), Failure> {
        self.control.check()
    }
    fn charge_work(&self, units: usize) -> Result<(), Failure> {
        self.control.charge_work(units)
    }
    fn charge_allocation(&self, bytes: usize) -> Result<(), Failure> {
        self.control.charge_allocation(bytes)?;
        self.allocated.fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    }
    fn release_allocation(&self, bytes: usize) {
        self.allocated.fetch_sub(bytes, Ordering::Relaxed);
        self.control.release_allocation(bytes);
    }
    fn allocation_limit_reached(&self) {
        self.control.allocation_limit_reached();
    }
    fn allocation_is_exhausted(&self) -> bool {
        self.control.allocation_is_exhausted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings_internal_json::work;

    #[test]
    fn backend_and_explicit_work_share_one_nonrenewable_allowance() {
        let failure = Attempt::run(10, 100, &|| false, |attempt| {
            attempt.charge_work(4).unwrap();
            assert_eq!(work::scope(100, || work::charge(5)), Ok(true));
            assert!(attempt.charge_work(2).is_err());
            // A discarded refusal cannot turn preparation into a success.
            true
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-work-units");
        assert!(
            Attempt::run(10, 100, &|| false, |a| {
                a.charge_work(10).unwrap();
                true
            })
            .unwrap()
        );
    }

    #[test]
    fn allocation_is_aggregate_checked_and_sticky() {
        for maximum in [10, usize::MAX] {
            let failure = Attempt::run(100, maximum, &|| false, |a| {
                a.charge_allocation(maximum).unwrap();
                assert_eq!(a.allocated(), maximum);
                assert!(a.charge_allocation(1).is_err());
                assert!(a.charge_allocation(0).is_err());
            })
            .unwrap_err();
            assert_eq!(failure.code, "preparation-sdk-reservation-bytes");
        }
        assert!(
            Attempt::run(100, 10, &|| false, |a| a.charge_allocation(10))
                .unwrap()
                .is_ok()
        );
    }

    #[test]
    fn nested_attempts_cannot_reset_allocation_or_backend_work() {
        let failure = Attempt::run(100, 10, &|| false, |outer| {
            outer.charge_allocation(6).unwrap();
            let nested = Attempt::run(100, 100, outer, |inner| inner.charge_allocation(5));
            assert!(nested.is_err());
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-reservation-bytes");
        let failure = Attempt::run(10, 100, &|| false, |outer| {
            outer.charge_work(5).unwrap();
            let nested = Attempt::run(100, 100, outer, |inner| inner.charge_work(10));
            assert!(nested.is_err());
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-work-units");
    }

    #[test]
    fn an_ignored_tighter_child_allocation_failure_stops_the_parent() {
        let failure = Attempt::run(1000, 1000, &|| false, |outer| {
            let child = Attempt::run(1000, 10, outer, |inner| inner.charge_allocation(11));
            assert!(child.is_err());
            assert!(outer.check().is_err());
            true
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-reservation-bytes");
    }

    #[test]
    fn parent_allocation_exhaustion_stops_the_current_child_checks() {
        let failure = Attempt::run(1000, 10, &|| false, |outer| {
            let child = Attempt::run(1000, 1000, outer, |inner| {
                assert!(inner.charge_allocation(11).is_err());
                assert!(inner.check().is_err());
            });
            assert!(child.is_err());
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-reservation-bytes");
    }

    #[test]
    fn identity_reservation_overflow_remains_sticky_if_its_error_is_ignored() {
        let failure = Attempt::run(100, usize::MAX, &|| false, |a| {
            assert!(identities(usize::MAX, a).is_err());
            assert!(a.check().is_err());
            true
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-reservation-bytes");
    }

    #[test]
    fn cancellation_is_not_budget_exhaustion_and_retry_is_independent() {
        let cancelled = AtomicBool::new(false);
        let failure = Attempt::run(0, 0, &cancelled, |a| {
            cancelled.store(true, Ordering::Release);
            assert_eq!(a.check().unwrap_err().reason, FailureReason::Cancelled);
        })
        .unwrap_err();
        assert_eq!(failure.reason, FailureReason::Cancelled);
        assert!(
            Attempt::run(1, 0, &|| false, |a| a.check())
                .unwrap()
                .is_ok()
        );
    }

    #[test]
    fn an_unwind_restores_and_debits_enclosing_backend_work() {
        assert_eq!(
            work::scope(10, || {
                let caught = std::panic::catch_unwind(|| {
                    let _ = Attempt::run(100, 100, &|| false, |a| {
                        a.charge_work(7).unwrap();
                        panic!("deliberate preparation interruption");
                    });
                });
                assert!(caught.is_err());
                assert!(work::charge(3));
            }),
            Ok(())
        );
    }
}
