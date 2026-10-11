//! Optional source-inspection role, separate from normative document semantics.
//!
//! Implement [`SourceInspector`] in a kind adapter; consume it through [`Inspector`]
//! to check carriers, support, cancellation and returned binding fragments.
//! Providers own acquisition and interpretation. No network client, executor,
//! registry, synthesizer or invoker is required.
//!
//! See the crate README for a complete implementer example and the role mapping.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod error;
mod inspector;
mod model;

pub use error::{InspectionError, InspectionErrorKind};
pub use inspector::{Inspector, SourceInspector};
pub use model::{
    BindableTarget, BindingFragment, InspectionLimitation, KindSupportVerdict, SourceInspection,
    SupportedKind,
};

/// Applied Source Inspector interface version, independent of the crate version.
pub const ROLE_VERSION: &str = "0.1.0";
/// Exact interfaces repository revision used by this companion.
pub const APPLIED_ROLE_REVISION: &str = "8ff5d9d913cb525dacb347a29e534af278ed88f6";
