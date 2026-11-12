#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod budget;
mod directional;
mod error;
mod input;
mod interface;
mod merge;
mod normalize;
mod reference;
mod reference_identity;
mod value;

pub use budget::ComparisonLimits;
pub use directional::{Comparison, Direction, Verdict, compare, compare_with_control};
pub use error::ComparisonFailure;
pub use input::{ComparisonContext, SchemaInput};
pub use interface::{
    InterfaceComparison, InterfaceIssue, InterfaceIssueKind, OperationComparison, OperationRole,
    check_interface, check_interface_with_control,
};
pub use reference_identity::{
    ReferenceContext, identical_with_references, identical_with_references_and_control,
};

/// Normalize every present input, then compare exact profile identity. A failed
/// present input fails even opposite an absent slot. No resource is fetched.
/// `true` means normalized identity (including two absent slots); `false` means
/// difference. A failure provides no identity verdict.
pub fn identical(
    left: Option<&SchemaInput>,
    right: Option<&SchemaInput>,
    limits: &ComparisonLimits,
) -> Result<bool, ComparisonFailure> {
    identical_with_control(
        left,
        right,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Compare with one shared work and memory policy and cooperative caller control.
pub fn identical_with_control(
    left: Option<&SchemaInput>,
    right: Option<&SchemaInput>,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<bool, ComparisonFailure> {
    let mut attempt = budget::Attempt::new(*limits, control);
    attempt.check()?;
    let left = left
        .map(|input| normalize::normalize(input, &mut attempt))
        .transpose()?;
    let right = right
        .map(|input| normalize::normalize(input, &mut attempt))
        .transpose()?;
    let equal = match (&left, &right) {
        (None, None) => true,
        (Some(left), Some(right)) => value::equal(left, right, 0, &mut attempt)?,
        _ => false,
    };
    attempt.check()?;
    Ok(equal)
}

/// Identifier of the independently adopted comparison profile.
pub const PROFILE_IDENTIFIER: &str = "OB-2020-12";
/// Version of the comparison profile, independent of SDK and core spec versions.
pub const PROFILE_VERSION: &str = "0.1";
/// Exact publication used to define this companion's comparison rules.
pub const APPLIED_INTERFACE_REVISION: &str = "484c9399207de99bd56921cead5f44a7b5740d9e";
