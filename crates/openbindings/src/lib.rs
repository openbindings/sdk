//! The normative OpenBindings document model and conformance boundary.
//!
//! ```
//! use openbindings::{DocumentBuilder, Operation, Conformance};
//! let mut draft = DocumentBuilder::new();
//! draft.operations.insert("lookup".into(), Operation::default());
//! let snapshot = draft.build()?;
//! let assessment = snapshot.assess()?;
//! assert_eq!(assessment.report().conclusion, Conformance::Conformant);
//! let validated = assessment.validated().expect("conformance was established");
//! assert!(validated.parsed().resolve_operation("lookup")?.is_some());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]
mod uri;
mod version;
pub use openbindings_internal_json::{
    InputError, InputErrorKind, JsonKind, JsonLimits, JsonMember, JsonRef, JsonValue,
    SourceLocation,
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
pub use schema_space::{Reference, ReferenceReport, ReferenceResolution};
