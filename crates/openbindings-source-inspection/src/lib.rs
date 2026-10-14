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
pub const APPLIED_ROLE_REVISION: &str = "f158f4f345301668ccb8fd555586b04ca823e732";

// One token/future identity across all asynchronous companions.
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
