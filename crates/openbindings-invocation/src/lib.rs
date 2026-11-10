// A method that returns Self is a builder step; dropping it loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod binding;
mod boundary;
mod channel;
mod error;
pub mod operation;
mod value;

pub use binding::{BindingInvoker, BindingInvokerExt, InvocationSupport};
pub use boundary::{BoundaryFailure, BoundaryFailureKind};
pub use channel::{
    Invocation, InvocationControl, InvocationInput, InvocationLimits, InvocationOutput, LimitError,
    OutputEvent, ProviderInput, ProviderIo, ProviderOutput, WriteError,
};
pub use error::InvocationError;
pub use openbindings_context::{Context, ContextChallenge};
pub use operation::{
    DefaultOperationInvoker, OperationAddress, OperationInvoker, OperationRequest,
};
pub use value::BindingRequest;

/// Version of the binding and operation invocation interfaces, separate from core.
pub const INTERFACE_VERSION: &str = "0.1.0";
/// Immutable current interfaces revision targeted by this implementation.
pub const APPLIED_INTERFACE_REVISION: &str = "3e3dc453a93b611a8e9ea1813ec63a2216830f45";

pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
