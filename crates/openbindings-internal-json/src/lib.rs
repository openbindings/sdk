#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
pub mod backend;
#[warn(missing_docs)]
mod json;
mod raw;
pub use json::*;
#[warn(missing_docs)]
mod pointer;
pub use pointer::{Pointer, PointerError};

pub mod decimal;
pub mod numeric;
pub mod text;
#[cfg(feature = "evaluator-work")]
#[warn(missing_docs)]
pub mod work;

#[warn(missing_docs)]
mod conversion;
pub use conversion::{ValueConversionError, ValueConversionErrorKind};
