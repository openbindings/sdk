//! Synchronous cooperative cancellation observation.
use crate::{NoVerdict, NoVerdictReason};
use std::{
    fmt,
    sync::atomic::{AtomicBool, Ordering},
};

/// The caller cancelled this work. The owning API maps this signal to its outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;
impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the caller cancelled this work")
    }
}
impl std::error::Error for Cancelled {}
impl From<Cancelled> for NoVerdict {
    fn from(_: Cancelled) -> Self {
        Self::new(
            NoVerdictReason::Cancelled,
            "caller-cancelled",
            "the caller cancelled this operation",
        )
    }
}

/// Object-safe synchronous observation of cooperative cancellation.
/// Implementations must remain cancelled for the rest of a work attempt. Resetting
/// a shared flag during work is unsupported; use a fresh flag for an independent
/// retry. Applications own scheduling and may compose signals such as deadlines.
/// Synchronous Wasm cannot observe a same-thread AbortSignal until it yields;
/// a worker is required for externally interruptible synchronous work.
///
/// ```
/// use openbindings::{Cancelled, WorkControl};
/// use std::sync::atomic::{AtomicBool, Ordering};
/// let flag = AtomicBool::new(false);
/// let control: &dyn WorkControl = &flag;
/// assert_eq!(control.check(), Ok(()));
/// flag.store(true, Ordering::Release);
/// assert_eq!(control.check(), Err(Cancelled));
/// ```
pub trait WorkControl: Send + Sync {
    /// Observe whether the caller has permanently cancelled this attempt.
    fn is_cancelled(&self) -> bool;
    /// Check cancellation at a cooperative work boundary.
    ///
    /// # Errors
    /// Returns `Cancelled` once the caller has cancelled this work attempt.
    fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}
impl WorkControl for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Acquire)
    }
}
pub(crate) struct NeverCancelled;
impl WorkControl for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl fmt::Debug for dyn WorkControl + '_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkControl").finish_non_exhaustive()
    }
}
