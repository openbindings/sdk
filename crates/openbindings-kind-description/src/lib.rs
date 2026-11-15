#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod error;
mod model;
mod queries;
pub use error::{CheckFailure, CheckRejection, KindError};
pub use model::{
    CarrierError, CarrierErrorKind, CheckDescription, CheckSourceRequest, CheckedSource,
    DescribeKindRequest, InvokeDescription, KindDescription, KindSummary, PullDescription,
};
pub use openbindings_context::{Context, ContextChallenge};
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
pub use queries::{
    DescriptionLimits, KindDescriptor, KindDescriptorExt, SourceChecker, SourceCheckerExt,
};

/// Version of the adopted kind-descriptor interface, independent of SDK version.
pub const INTERFACE_VERSION: &str = "0.1.0";
/// Exact interface publication used by this companion.
pub const APPLIED_INTERFACE_REVISION: &str = "eafa49a14539711f3c9d8d9d4dbcb494ea4eb7c1";
