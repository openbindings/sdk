use openbindings::{ReferenceDefect, ResolutionFailure, ResolutionFailureKind};
const SECRET: &str = "SENTINEL-secret-7391";
#[test]
fn lookup_failures_show_only_fixed_categories() {
    let mut cases = vec![
        (
            ResolutionFailureKind::ResourceUnavailable { uri: SECRET.into() },
            "resource is unavailable",
            "ResourceUnavailable",
        ),
        (
            ResolutionFailureKind::UnsupportedDialect { uri: SECRET.into() },
            "unsupported dialect",
            "UnsupportedDialect",
        ),
        (
            ResolutionFailureKind::UnsupportedCapability {
                capability: SECRET.into(),
            },
            "unsupported capability",
            "UnsupportedCapability",
        ),
        (
            ResolutionFailureKind::InvalidExternalSchema,
            "invalid identity or dialect",
            "InvalidExternalSchema",
        ),
        (
            ResolutionFailureKind::Limit {
                resource: SECRET.into(),
            },
            "reached a limit",
            "Limit",
        ),
        (
            ResolutionFailureKind::Cancelled,
            "was cancelled",
            "Cancelled",
        ),
        (
            ResolutionFailureKind::WrongEnvironment,
            "different environment",
            "WrongEnvironment",
        ),
        (
            ResolutionFailureKind::NotSchemaLocation,
            "outside the schema inventory",
            "NotSchemaLocation",
        ),
        (
            ResolutionFailureKind::InvalidAnchorName,
            "dynamic-anchor name is invalid",
            "InvalidAnchorName",
        ),
    ];
    for (defect, text) in [
        (ReferenceDefect::MalformedReference, "is malformed"),
        (ReferenceDefect::MissingTarget, "target is missing"),
        (ReferenceDefect::NonSchemaTarget, "target is not a schema"),
        (ReferenceDefect::MissingAnchor, "anchor is missing"),
        (ReferenceDefect::AmbiguousAnchor, "anchor is ambiguous"),
        (
            ReferenceDefect::InvalidIdentifier,
            "no usable base identifier",
        ),
    ] {
        cases.push((ResolutionFailureKind::Undefined(defect), text, "Undefined"));
    }
    for (kind, message, name) in cases {
        let failure = ResolutionFailure { kind, origin: None };
        let display = failure.to_string();
        let debug = format!("{failure:?}");
        assert!(display.contains(message), "{display}");
        assert!(debug.contains(name), "{debug}");
        assert!(!display.contains(SECRET));
        assert!(!debug.contains(SECRET));
    }
}
