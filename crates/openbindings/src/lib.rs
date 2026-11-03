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

mod authoring;
pub use authoring::*;

pub mod conformance;
pub mod document;
pub use conformance::*;
mod fixed_schema;
mod schema_index;
pub use document::*;

pub mod schema;
pub use schema::Schema;

mod contracts;
pub use contracts::*;
mod work_control;
pub use work_control::*;
mod schema_space;
pub use schema_space::{
    Reference, ReferenceRefusal, ReferenceRefusalReason, ReferenceReport, ReferenceResolution,
};
