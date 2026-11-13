use crate::ComparisonFailure;
use openbindings::WorkControl;

/// Local memory policy and optional work budget, including failed work.
#[derive(Clone, Copy, Debug)]
pub struct ComparisonLimits {
    /// Optional cooperative normalization/comparison work cap; None by default.
    /// A time bound is the caller's choice, through WorkControl cancellation or
    /// this field. Work units are not a portable duration or a hard deadline.
    pub max_work: Option<usize>,
    /// Conservative reservations for storage held at once; default 64 MiB.
    /// Scratch is released after each slot; shared indexes, normalized targets
    /// and retained reports stay charged. Estimates are not allocator accounting.
    pub max_allocation_bytes: usize,
    /// Maximum normalization/comparison nesting; default and fixed ceiling 128.
    /// Smaller values tighten the local policy; larger values do not raise the ceiling.
    pub max_depth: usize,
}
impl Default for ComparisonLimits {
    fn default() -> Self {
        Self {
            max_work: None,
            max_allocation_bytes: 64 * 1024 * 1024,
            max_depth: 128,
        }
    }
}
type NormalizedKey = (usize, (usize, usize), usize, usize);
type LocalResource = (usize, (usize, usize), crate::reference::Resources);
pub(crate) struct Attempt<'a> {
    pub(crate) contexts: Vec<crate::ComparisonContext>,
    pub(crate) normalized:
        std::collections::BTreeMap<NormalizedKey, std::sync::Arc<crate::value::Value>>,
    pub(crate) local_resources: Vec<LocalResource>,
    limits: ComparisonLimits,
    work: usize,
    allocation: usize,
    retained: usize,
    failure: Option<ComparisonFailure>,
    control: &'a dyn WorkControl,
}
impl<'a> Attempt<'a> {
    pub(crate) fn new(limits: ComparisonLimits, control: &'a dyn WorkControl) -> Self {
        Self {
            contexts: Vec::new(),
            normalized: Default::default(),
            local_resources: Vec::new(),
            limits,
            work: 0,
            allocation: 0,
            retained: 0,
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
        if self.limits.max_work.is_none() {
            return Ok(());
        }
        self.work = self
            .work
            .checked_add(amount)
            .ok_or_else(|| self.exhausted("work"))?;
        if self
            .limits
            .max_work
            .is_some_and(|maximum| self.work > maximum)
        {
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
    /// Indexes and shared normalized targets survive scratch scopes.
    pub(crate) fn retain(&mut self, bytes: usize) -> Result<(), ComparisonFailure> {
        self.allocate(bytes)?;
        self.retained += bytes;
        Ok(())
    }
    pub(crate) fn allocation_mark(&self) -> (usize, usize) {
        (self.allocation, self.retained)
    }
    /// Promote newly allocated storage when publishing a shared cached target.
    pub(crate) fn retain_since(&mut self, mark: (usize, usize)) {
        self.retained += (self.allocation - mark.0) - (self.retained - mark.1);
    }
    pub(crate) fn release(&mut self, bytes: usize) {
        self.allocation -= bytes;
    }
    /// The closure drops its scratch before returning; retained data stays charged.
    pub(crate) fn scratch<T>(&mut self, run: impl FnOnce(&mut Self) -> T) -> T {
        let mark = self.allocation_mark();
        let result = run(self);
        self.allocation = mark.0 + (self.retained - mark.1);
        result
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
            max_work: Some(usize::MAX),
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
