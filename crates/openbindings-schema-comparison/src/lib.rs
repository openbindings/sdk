#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod budget;
mod error;
mod input;
mod merge;
mod normalize;
mod reference;
mod value;

pub use budget::ComparisonLimits;
pub use error::ComparisonFailure;
pub use input::SchemaInput;

/// Identity after this profile's normalization, not general schema equivalence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    /// The two normalized optional schema slots are identical.
    Equal,
    /// The normalized slots differ, including absent versus present.
    Different,
}
/// Normalize every present input, then compare exact profile identity. A failed
/// present input fails even opposite an absent slot. No resource is fetched.
pub fn identical(
    left: Option<&SchemaInput>,
    right: Option<&SchemaInput>,
    limits: &ComparisonLimits,
) -> Result<Identity, ComparisonFailure> {
    identical_with_control(
        left,
        right,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
    )
}
/// Compare with one shared monotone allowance and cooperative caller control.
pub fn identical_with_control(
    left: Option<&SchemaInput>,
    right: Option<&SchemaInput>,
    limits: &ComparisonLimits,
    control: &dyn openbindings::WorkControl,
) -> Result<Identity, ComparisonFailure> {
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
    Ok(if equal {
        Identity::Equal
    } else {
        Identity::Different
    })
}

/// Identifier of the independently adopted comparison profile.
pub const PROFILE_IDENTIFIER: &str = "OB-2020-12";
/// Version of the comparison profile, independent of SDK and core spec versions.
pub const PROFILE_VERSION: &str = "0.1";
/// Exact publication used to define this companion's identity rules.
pub const APPLIED_INTERFACE_REVISION: &str = "5815fb106bddd2c85ff4e93e0077570b3f6b9e74";
