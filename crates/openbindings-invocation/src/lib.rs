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

/// Binding and Operation Invoker interface version, independent of the crate version.
pub const ROLE_VERSION: &str = "0.1.0";

/// Exact interfaces repository revision applied by this companion.
pub const APPLIED_ROLE_REVISION: &str = "f158f4f345301668ccb8fd555586b04ca823e732";

// One token/future identity across all asynchronous companions.
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
