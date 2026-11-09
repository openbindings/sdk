#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
pub mod json;
mod uri;
mod version;
pub use json::*;
pub use version::*;

pub mod conformance;
pub mod document;
pub use conformance::*;
mod fixed_schema;
pub use document::*;

pub mod schema;
pub use schema::*;

pub mod control;
pub use control::*;
