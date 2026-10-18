#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod uri;
mod version;
pub use openbindings_internal_json::{
    InputError, InputErrorKind, JsonKind, JsonLimits, JsonMember, JsonRef, JsonStringRef,
    JsonValue, Pointer, PointerError, SourceLocation, ValueConversionError,
    ValueConversionErrorKind,
};
pub use version::*;

mod authoring;
pub use authoring::*;

mod document;
mod fixed_schema;
mod schema_index;
pub use document::*;

mod contracts;
pub use contracts::*;
mod work_control;
pub use work_control::*;
mod schema_space;
pub use schema_space::{
    Reference, ReferenceRefusal, ReferenceRefusalReason, ReferenceReport, ReferenceResolution,
};
