//! The normative OpenBindings document model and conformance boundary.
//!
//! ```
//! use openbindings::{Document, Operation, Conformance};
//! let mut draft = Document::new();
//! draft.operations.insert("lookup".into(), Operation::default());
//! let snapshot = draft.build()?;
//! let validation = snapshot.validate()?;
//! assert_eq!(validation.report().conclusion, Conformance::Conformant);
//! let validated = validation.validated().expect("conformance was established");
//! assert!(matches!(validated.parsed().resolve_operation("lookup")?, openbindings::OperationSelection::Found(_)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod uri;
mod version;
pub use openbindings_internal_json::{
    InputError, InputErrorKind, JsonKind, JsonLimits, JsonMember, JsonRef, JsonStringRef,
    JsonValue, SourceLocation, ValueConversionError, ValueConversionErrorKind,
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
