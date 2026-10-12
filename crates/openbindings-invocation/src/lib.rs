//! Checked, executor-independent binding invocation for kind adapters.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod binding;
mod error;
mod lifecycle;
mod value;

pub use error::{BoundaryError, InvocationError};
pub use lifecycle::{
    Control, Input, Invocation, InvocationLimits, LimitError, Outputs, ProviderInput, ProviderIo,
    ProviderOutput, WriteError,
};
pub use value::{BindingInvocationInput, BindingRequest, BindingValue, ContextChallenge};

/// Binding Invoker interface version, independent of the crate version.
pub const ROLE_VERSION: &str = "0.1.0";

/// Exact interfaces repository revision applied by this companion.
pub const APPLIED_ROLE_REVISION: &str = "3972eb977f419f792b92fd61d5afa5fba4946b04";
