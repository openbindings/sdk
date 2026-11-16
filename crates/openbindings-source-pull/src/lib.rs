#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod budget;
mod carrier;
mod error;
mod model;
mod pull;
mod relationships;
mod request;
mod validation;
pub use budget::PullLimits;
pub use carrier::{CarrierError, CarrierErrorKind};
pub use error::{PullValidationError, RelationshipError, RelationshipErrorKind, RequestError};
pub use model::*;
pub use openbindings_context::{ChallengeShapeError, Context, ContextChallenge, ContextShapeError};
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
pub use pull::{PullError, PullFailure, SourcePuller, SourcePullerExt};
pub use request::PullRequest;

/// Adopted source-puller interface version, independent of the SDK version.
pub const INTERFACE_VERSION: &str = "0.1.0";
/// Exact interface publication used by this companion.
pub const APPLIED_INTERFACE_REVISION: &str = "8be6f6837f90da7091e5496a1686e34bf3b57c15";

/// Operation key for `pullSource` in the pinned source-puller interface.
pub const PULL_SOURCE: &str = "openbindings.source-puller.pullSource";
