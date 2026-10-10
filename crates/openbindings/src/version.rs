//! Declared-version policy. Numeric components have no machine-integer ceiling.
use serde::Serialize;

/// Supported stable specification line, `0.2.x`; independent of the crate version.
pub const SUPPORTED_VERSIONS: &str = "0.2.x";
/// Default declaration written by new authoring drafts, `0.2.0`.
pub const AUTHORING_VERSION: &str = "0.2.0";
/// Specification release whose document rules this build applies.
pub const APPLIED_SPEC_RELEASE: &str = "0.2.0";
/// Exact applied specification Git revision; identifies the rules behind assessment.
pub const APPLIED_SPEC_REVISION: &str = "2f7d754dc2da374058cd517064c17e50f7d95d99";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Declared-version classification, independent of document conformance.
pub enum VersionDecision {
    /// Well-formed stable version in the supported line; document rules still need assessment.
    Supported,
    /// The declaration is not syntactically valid SemVer.
    Malformed,
    /// Well-formed SemVer outside the supported line, including prereleases.
    Unsupported,
}

/// Supports every stable patch in the 0.2 line. Build metadata has no effect.
pub fn check_version(version: &str) -> VersionDecision {
    let Some((major, minor, prerelease)) = components(version) else {
        return VersionDecision::Malformed;
    };
    if major == "0" && minor == "2" && !prerelease {
        VersionDecision::Supported
    } else {
        VersionDecision::Unsupported
    }
}
fn numeric(s: &str) -> bool {
    !s.is_empty() && (s.len() == 1 || !s.starts_with('0')) && s.bytes().all(|b| b.is_ascii_digit())
}
fn identifiers(s: &str, prerelease: bool) -> bool {
    s.split('.').all(|p| {
        !p.is_empty()
            && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && (!prerelease || !p.bytes().all(|b| b.is_ascii_digit()) || numeric(p))
    })
}
fn components(version: &str) -> Option<(&str, &str, bool)> {
    let core = if let Some((core, build)) = version.split_once('+') {
        if !identifiers(build, false) {
            return None;
        }
        core
    } else {
        version
    };
    let (core, prerelease) = if let Some((core, pre)) = core.split_once('-') {
        if !identifiers(pre, true) {
            return None;
        }
        (core, true)
    } else {
        (core, false)
    };
    let mut parts = core.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some() || ![major, minor, patch].into_iter().all(numeric) {
        return None;
    }
    Some((major, minor, prerelease))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unbounded_components_and_exact_syntax() {
        for v in ["0.2.0", "0.2.9999999999999999999999999999999999+01.a-b"] {
            assert_eq!(check_version(v), VersionDecision::Supported);
        }
        for v in ["0.2.0-alpha", "0.3.0", "999999999999999999999999.2.0"] {
            assert_eq!(check_version(v), VersionDecision::Unsupported);
        }
        for v in [
            "0.02.0",
            "0.2.0\n",
            "0.2.0-01",
            "0.2.0+",
            "0.2.0+x+y",
            "0.2.0-a..b",
            "0.2",
        ] {
            assert_eq!(check_version(v), VersionDecision::Malformed, "{v}");
        }
    }
}
