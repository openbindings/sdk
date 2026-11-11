use crate::ComparisonFailure;
use openbindings::WorkControl;

/// Finite local work policy for a single comparison, including failed work.
#[derive(Clone, Copy, Debug)]
pub struct ComparisonLimits {
    /// Monotone work allowance for normalization and comparison; default 20 million.
    pub max_work: usize,
    /// Conservative cumulative allocation reservations; default 64 MiB.
    pub max_allocation_bytes: usize,
    /// Maximum normalization/comparison nesting; default and fixed ceiling 128.
    /// Smaller values tighten the local policy; larger values do not raise the ceiling.
    pub max_depth: usize,
}
impl Default for ComparisonLimits {
    fn default() -> Self {
        Self {
            max_work: 20_000_000,
            max_allocation_bytes: 64 * 1024 * 1024,
            max_depth: 128,
        }
    }
}
pub(crate) struct Attempt<'a> {
    limits: ComparisonLimits,
    work: usize,
    allocation: usize,
    failure: Option<ComparisonFailure>,
    control: &'a dyn WorkControl,
}
impl<'a> Attempt<'a> {
    pub(crate) fn new(limits: ComparisonLimits, control: &'a dyn WorkControl) -> Self {
        Self {
            limits,
            work: 0,
            allocation: 0,
            failure: None,
            control,
        }
    }
    pub(crate) fn check(&self) -> Result<(), ComparisonFailure> {
        if self.control.is_cancelled() {
            Err(ComparisonFailure::Cancelled)
        } else if let Some(failure) = self.failure {
            Err(failure)
        } else {
            Ok(())
        }
    }
    fn exhausted(&mut self, resource: &'static str) -> ComparisonFailure {
        let failure = ComparisonFailure::LimitExceeded(resource);
        self.failure = Some(failure);
        failure
    }
    pub(crate) fn work(&mut self, amount: usize) -> Result<(), ComparisonFailure> {
        self.check()?;
        self.work = self
            .work
            .checked_add(amount)
            .ok_or_else(|| self.exhausted("work"))?;
        if self.work > self.limits.max_work {
            Err(self.exhausted("work"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn allocate(&mut self, bytes: usize) -> Result<(), ComparisonFailure> {
        self.check()?;
        self.allocation = self
            .allocation
            .checked_add(bytes)
            .ok_or_else(|| self.exhausted("allocation-bytes"))?;
        if self.allocation > self.limits.max_allocation_bytes {
            Err(self.exhausted("allocation-bytes"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn depth(&mut self, depth: usize) -> Result<(), ComparisonFailure> {
        self.work(1)?;
        if depth > self.limits.max_depth.min(128) {
            Err(self.exhausted("depth"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, ComparisonFailure> {
        self.check()?;
        let bytes = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| self.exhausted("allocation-bytes"))?;
        self.allocate(bytes)?;
        Ok(Vec::with_capacity(count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[test]
    fn every_resource_refusal_remains_sticky() {
        let control = AtomicBool::new(false);
        let limits = ComparisonLimits {
            max_work: usize::MAX,
            max_allocation_bytes: usize::MAX,
            max_depth: 128,
        };
        for route in 0..5 {
            let mut attempt = Attempt::new(limits, &control);
            let refusal = match route {
                0 => {
                    attempt.work(usize::MAX).unwrap();
                    attempt.work(1)
                }
                1 => {
                    attempt.allocate(usize::MAX).unwrap();
                    attempt.allocate(1)
                }
                2 => attempt.depth(129),
                3 => attempt.vector::<u64>(usize::MAX).map(|_| ()),
                _ => {
                    attempt.limits.max_allocation_bytes = 0;
                    attempt.allocate(1)
                }
            }
            .unwrap_err();
            assert_eq!(attempt.check(), Err(refusal));
            assert_eq!(attempt.work(0), Err(refusal));
            assert_eq!(attempt.allocate(0), Err(refusal));
            assert_eq!(attempt.depth(0), Err(refusal));
            assert_eq!(attempt.vector::<u8>(0), Err(refusal));
            assert_eq!(attempt.vector::<u64>(usize::MAX), Err(refusal));
        }
    }
}
