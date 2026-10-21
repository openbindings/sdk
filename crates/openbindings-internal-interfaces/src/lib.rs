#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod binding;
pub mod carrier;
mod kinds;
pub use binding::BindingValue;
pub use carrier::BoundaryError;
pub use kinds::{KindSupportVerdict, SupportedKind, kind_support_verdicts};
