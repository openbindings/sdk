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
pub use carrier::CarrierError;
pub use error::{PullValidationError, RelationshipError, RequestError};
pub use model::*;
pub use openbindings_context::{Context, ContextChallenge};
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
pub use pull::{PullError, PullFailure, SourcePuller, SourcePullerExt};
pub use request::PullRequest;

/// Adopted source-puller interface version, independent of the SDK version.
pub const INTERFACE_VERSION: &str = "0.1.0";
/// Exact interface publication used by this companion.
pub const APPLIED_INTERFACE_REVISION: &str = "67f707c31aecbdf0551385e369d8024097db6278";
