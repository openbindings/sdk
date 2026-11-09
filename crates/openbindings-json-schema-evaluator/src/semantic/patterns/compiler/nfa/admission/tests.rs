use super::*;
use openbindings_internal_schema::{Failure, FailureReason, preparation::Attempt};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn parse(form: &str) -> Hir {
    regex_syntax::Parser::new().parse(form).unwrap()
}

#[test]
fn capped_arithmetic_preserves_zero_and_never_wraps() {
    for a in [0, 1, SENTINEL - 1, SENTINEL, usize::MAX] {
        for b in [0, 1, SENTINEL - 1, SENTINEL, usize::MAX] {
            let expected_add = ((a as u128) + b as u128).min(SENTINEL as u128) as usize;
            let expected_mul = ((a as u128) * b as u128).min(SENTINEL as u128) as usize;
            assert_eq!(add(a, b), expected_add);
            assert_eq!(multiply(a, b), expected_mul);
        }
    }
}

#[test]
fn cancellation_during_unicode_and_ancestor_work_never_publishes_scores() {
    let hir = parse("([é-ê][Ā-ą]|[😀-🙏]){2,3}");
    let calls = AtomicUsize::new(0);
    inspect(&hir, &|| {
        calls.fetch_add(1, Relaxed);
        false
    })
    .unwrap();
    let boundaries = calls.load(Relaxed);
    for boundary in 0..boundaries {
        calls.store(0, Relaxed);
        let cancel = || calls.fetch_add(1, Relaxed) >= boundary;
        assert!(matches!(
            inspect(&hir, &cancel),
            Err(Error::Attempt(Failure {
                reason: FailureReason::Cancelled,
                ..
            }))
        ));
    }
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        let failed = inspect(&hir, attempt);
        assert!(matches!(failed, Err(Error::Attempt(_))));
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
    inspect(&hir, &|| false).unwrap();
}

#[test]
fn an_unreviewed_look_mode_cannot_inherit_the_closed_profile() {
    assert!(matches!(
        inspect(&Hir::look(Look::WordUnicode), &|| false),
        Err(Error::Representation)
    ));
}
