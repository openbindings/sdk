#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod error;
mod model;
mod queries;
pub use error::{CheckFailure, CheckRejection, KindError};
pub use model::{
    CarrierError, CheckDescription, CheckSourceRequest, CheckedSource, DescribeKindRequest,
    KindDescription, KindSummary,
};
pub use openbindings_context::{Context, ContextChallenge};
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
pub use queries::{
    DescriptionLimits, KindDescriptor, KindDescriptorExt, SourceChecker, SourceCheckerExt,
};

/// Version of the adopted kind-descriptor interface, independent of SDK version.
pub const INTERFACE_VERSION: &str = "0.1.0";
/// Exact interface publication used by this companion.
pub const APPLIED_INTERFACE_REVISION: &str = "5815fb106bddd2c85ff4e93e0077570b3f6b9e74";
