#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod binding;
mod error;
mod lifecycle;
pub mod operation;
mod value;

pub use error::{BoundaryError, InvocationError};
pub use lifecycle::{
    Control, Input, Invocation, InvocationLimits, LimitError, Outputs, ProviderInput, ProviderIo,
    ProviderOutput, WriteError,
};
pub use value::{BindingInvocationInput, BindingRequest, BindingValue, ContextChallenge};

/// Version of the published interface this crate implements, independent of the crate's own version.
pub const INTERFACE_VERSION: &str = "0.1.0";

/// Revision of the interfaces repository applied by this crate.
pub const APPLIED_INTERFACE_REVISION: &str = "f158f4f345301668ccb8fd555586b04ca823e732";

// One token/future identity across all asynchronous companions.
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};

#[cfg(test)]
mod interface_metadata_tests {
    #[test]
    fn published_interface_metadata_is_independent_of_the_crate_version() {
        // The published interface version and applied revision are separate from
        // the crate version, so a crate release cannot silently change them.
        assert_eq!(super::INTERFACE_VERSION, "0.1.0");
        assert_eq!(
            super::APPLIED_INTERFACE_REVISION,
            "f158f4f345301668ccb8fd555586b04ca823e732"
        );
    }
}
