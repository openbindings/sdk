#![doc = include_str!("../README.md")]
mod error;
mod exact;
mod model;
mod synthesizer;
mod validate;
pub use error::*;
pub use model::*;
pub use synthesizer::*;
/// Applied Interface Synthesizer role version, independent of the SDK version.
pub const APPLIED_ROLE_VERSION: &str = "0.2.0";
/// Exact interfaces revision applied by this companion.
pub const APPLIED_ROLE_REVISION: &str = "56aadc07aef362d1bbe1f42dd3081f4a750555f6";
