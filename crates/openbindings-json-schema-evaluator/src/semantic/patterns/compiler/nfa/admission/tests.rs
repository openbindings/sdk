use super::*;
use openbindings_internal_schema::{Failure, FailureReason, preparation::Attempt};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn parse(form: &str) -> Hir {
    regex_syntax::Parser::new().parse(form).unwrap()
}

#[test]
fn frozen_independent_triplets_and_profile_classification() {
    let controls: serde_json::Value =
        serde_json::from_str(include_str!("independent.json")).unwrap();
    for row in controls.as_array().unwrap() {
        let hir = match row["literal_len"].as_u64() {
            Some(n) => Hir::literal(vec![b'a'; n as usize]),
            None => parse(row["form"].as_str().unwrap()),
        };
        let scores = inspect(&hir, &|| false).unwrap();
        assert_eq!(
            serde_json::json!({"dispatch":scores.dispatch,"helper":scores.helper_payload,"tail_pressure":scores.empty_tail_pressure}),
            row["expected"],
            "{}",
            row["id"]
        );
        let expected = &row["expected"];
        assert_eq!(
            scores.admitted(),
            expected["dispatch"].as_u64().unwrap() <= 65_536
                && expected["helper"].as_u64().unwrap() <= 1_048_576
                && expected["tail_pressure"].as_u64().unwrap() <= 1_048_576
        );
    }
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
    let excessive = Facts {
        dispatch: SENTINEL,
        helper: SENTINEL,
        tail: Tail {
            prefix: SENTINEL,
            suffix: SENTINEL,
            maximum: SENTINEL,
            pure: true,
            unions: SENTINEL,
        },
    };
    let repetition = Repetition {
        min: 0,
        max: Some(0),
        greedy: true,
        sub: Box::new(Hir::empty()),
    };
    let zero = excessive.repeat(&repetition);
    assert_eq!(
        (
            zero.dispatch,
            zero.helper,
            zero.tail.prefix,
            zero.tail.unions
        ),
        (1, 0, 1, 0)
    );
}

#[derive(Default)]
struct Reservations {
    utf8: AtomicUsize,
    bytes: AtomicUsize,
}
impl Control for Reservations {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn charge_allocation(&self, bytes: usize) -> Result<(), Failure> {
        self.bytes.fetch_add(bytes, Relaxed);
        if bytes == UTF8_SCRATCH_BYTES {
            self.utf8.fetch_add(1, Relaxed);
        }
        Ok(())
    }
}
#[test]
fn unicode_inventory_reuses_one_scratch_owner_and_ascii_allocates_none() {
    let control = Reservations::default();
    inspect(&parse("[a-z]"), &control).unwrap();
    assert_eq!(control.bytes.load(Relaxed), 0);
    let hir = parse("[é-ê][Ā-ą][😀-🙏]");
    inspect(&hir, &control).unwrap();
    assert_eq!(control.utf8.load(Relaxed), 1);
    // A fresh attempt gets a fresh owner; no iterator is retained by HIR.
    inspect(&hir, &control).unwrap();
    assert_eq!(control.utf8.load(Relaxed), 2);
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
