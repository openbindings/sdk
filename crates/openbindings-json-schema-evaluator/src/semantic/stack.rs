//! Sampled live stack use for one synchronous original-engine call.
//!
//! This is an additional refusal guard, not a hard process stack bound: helper
//! peaks between samples still require separate layout and host qualification.
use super::{NoVerdict, decision};
pub(super) struct Observation {
    origin: usize,
    bytes: usize,
    exceeded: bool,
}
impl Observation {
    pub(super) fn new(origin: usize, bytes: usize) -> Self {
        Self {
            origin,
            bytes,
            exceeded: false,
        }
    }
    #[inline(never)]
    pub(super) fn check(&mut self) -> Result<(), NoVerdict> {
        let marker = 0u8;
        let here = std::hint::black_box(std::ptr::addr_of!(marker)) as usize;
        self.exceeded |= self.origin.abs_diff(here) > self.bytes;
        if self.exceeded {
            Err(decision::limit("evaluation-stack-limit"))
        } else {
            Ok(())
        }
    }
}
