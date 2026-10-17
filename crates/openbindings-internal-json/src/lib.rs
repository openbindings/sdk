#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
pub mod backend;
#[warn(missing_docs)]
mod json;
mod raw;
pub use json::*;

pub mod decimal;
pub mod numeric;
pub mod text;

#[warn(missing_docs)]
mod conversion;
pub use conversion::{ValueConversionError, ValueConversionErrorKind};
