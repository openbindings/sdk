//! Checked, executor-independent binding and operation invocation for kind adapters.
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
pub const APPLIED_ROLE_REVISION: &str = "8b25ddccb477db31d7a507fe07227e08f544679d";
