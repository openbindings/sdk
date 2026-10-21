#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod kinds;
pub use kinds::{KindSupportVerdict, SupportedKind, kind_support_verdicts};
