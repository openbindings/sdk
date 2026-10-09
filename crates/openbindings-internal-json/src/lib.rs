//! Implementation support shared by the core and optional evaluator.
//! Consumers use the exact-value APIs re-exported by `openbindings`.
#![forbid(unsafe_code)]
pub mod backend;
mod json;
mod raw;
pub use json::*;

pub mod numeric;

mod conversion;
pub use conversion::{ValueConversionError, ValueConversionErrorKind};
