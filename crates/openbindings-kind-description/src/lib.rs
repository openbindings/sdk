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
pub use openbindings_context::{ChallengeShapeError, Context, ContextChallenge, ContextShapeError};
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
pub use queries::{
    DescriptionLimits, KindDescriptor, KindDescriptorExt, SourceChecker, SourceCheckerExt,
};

/// Version of the adopted kind-descriptor interface, independent of SDK version.
pub const INTERFACE_VERSION: &str = "0.1.0";
/// Exact interface publication used by this companion.
pub const APPLIED_INTERFACE_REVISION: &str = "8be6f6837f90da7091e5496a1686e34bf3b57c15";

/// Operation key for `listKinds` in the pinned kind-descriptor interface.
pub const LIST_KINDS: &str = "openbindings.kind-descriptor.listKinds";
/// Operation key for `describeKind` in the pinned kind-descriptor interface.
pub const DESCRIBE_KIND: &str = "openbindings.kind-descriptor.describeKind";
/// Operation key for `checkSource` in the pinned kind-descriptor interface.
pub const CHECK_SOURCE: &str = "openbindings.kind-descriptor.checkSource";
