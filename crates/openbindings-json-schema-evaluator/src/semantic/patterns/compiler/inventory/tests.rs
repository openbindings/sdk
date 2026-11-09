use super::super::ast;
use super::*;
use openbindings_internal_schema::{FailureReason, preparation::Attempt};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn parsed(form: &str) -> Ast {
    ast::prepare(
        form,
        32,
        250,
        &super::super::profile::Budget::default(),
        &|| false,
    )
    .unwrap()
}
#[test]
fn arithmetic_refusal_is_local_but_preserves_a_prior_shared_cause() {
    assert!(matches!(
        checked(Amount::from(usize::MAX) + 1),
        Err(InternalError::Overflow)
    ));
    assert!(matches!(
        checked(Amount::from(usize::MAX) * 2),
        Err(InternalError::Overflow)
    ));
    assert!(matches!(
        map_error(InternalError::Overflow, &|| false),
        Error::ConstructionLimit
    ));
    let result = Attempt::run(100, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert!(matches!(
            map_error(InternalError::Overflow, attempt),
            Error::Attempt(Failure {
                reason: FailureReason::LimitExceeded,
                ..
            })
        ));
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
}
#[test]
fn cancellation_never_publishes_a_partial_certificate() {
    let ast = parsed("(a|b)*");
    assert!(matches!(
        inspect(ast, &|| true),
        Err(Error::Attempt(Failure {
            reason: FailureReason::Cancelled,
            ..
        }))
    ));
    let counter = AtomicUsize::new(0);
    for boundary in 1..30 {
        counter.store(0, Relaxed);
        let ast = parsed("((((a|b)*)))");
        let cancelled = || counter.fetch_add(1, Relaxed) >= boundary;
        let result = Attempt::run(usize::MAX, usize::MAX, &cancelled, |attempt| {
            inspect(ast, attempt)
        });
        if let Err(failure) = result {
            assert_eq!(failure.reason, FailureReason::Cancelled);
        }
    }
    drop(inspect(parsed("(a|b)*"), &|| false).unwrap());
}
#[test]
fn original_forms_preserve_ast_and_candidate_admission() {
    let mut count = 0;
    let mut attacks = 0;
    for line in include_str!("../normalized/tests/reader-inputs.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let Ok(crate::pattern::Reading::Linear(reading)) =
            crate::pattern::read(row["pattern"].as_str().unwrap(), 32, 1024 * 1024)
        else {
            continue;
        };
        let parsed = ast::prepare(
            &reading.form,
            33,
            250,
            &super::super::profile::Budget::default(),
            &|| false,
        );
        if matches!(row["id"].as_u64(), Some(1184 | 1185)) {
            assert!(matches!(
                parsed,
                Err(ast::Error::Refused(ast::Refusal::ConstructionLimit))
            ));
            attacks += 1;
            continue;
        }
        let owned = inspect(parsed.unwrap(), &|| false).unwrap();
        assert_eq!(owned.tree.span().end.offset, reading.form.len());
        assert!(
            super::super::profile::admits_translation(&owned.summary.pressure),
            "{}",
            row["id"]
        );
        count += 1;
    }
    assert_eq!((count, attacks), (1140, 2));
}
