use openbindings_source_pull::{PullError, PullFailure, PullValidationError, RequestError};

// Positive controls for the four compile-fail exhaustive-match rustdoc probes.
fn request(error: RequestError) -> u8 {
    match error {
        RequestError::Shape(_) => 1,
        RequestError::Interface(_) => 2,
        RequestError::Version(_) => 3,
        _ => 0,
    }
}
fn validation(error: PullValidationError) -> u8 {
    match error {
        PullValidationError::MissingSource => 1,
        PullValidationError::Relationship(_) => 2,
        PullValidationError::LimitExceeded(_) => 3,
        PullValidationError::Cancelled => 4,
        PullValidationError::Edit(_) => 5,
        PullValidationError::Prospective(_) => 6,
        PullValidationError::Version(_) => 7,
        _ => 0,
    }
}
fn provider(error: PullFailure<()>) -> u8 {
    match error {
        PullFailure::ContextRequired(_) => 1,
        PullFailure::Operational(_) => 2,
        PullFailure::Cancelled => 3,
        _ => 0,
    }
}
fn checked(error: PullError<()>) -> u8 {
    match error {
        PullError::Provider(_) => 1,
        PullError::Validation(_) => 2,
        PullError::Cancelled => 3,
        _ => 0,
    }
}

#[test]
fn downstream_matches_with_unknown_failure_policies_compile() {
    assert_eq!(request(RequestError::Shape("source")), 1);
    assert_eq!(validation(PullValidationError::MissingSource), 1);
    assert_eq!(provider(PullFailure::Cancelled), 3);
    assert_eq!(checked(PullError::Cancelled), 3);
}
