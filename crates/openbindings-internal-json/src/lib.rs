//! Implementation support shared by the core and optional evaluator.
//! Consumers use the exact-value APIs re-exported by `openbindings`.
#![forbid(unsafe_code)]
pub mod backend;
#[warn(missing_docs)]
mod json;
mod raw;
pub use json::*;

pub mod numeric;

#[warn(missing_docs)]
mod conversion;
pub use conversion::{ValueConversionError, ValueConversionErrorKind};
