use super::*;
use openbindings_internal_schema::preparation::Attempt;

#[test]
fn each_aggregate_is_inclusive_and_sticky() {
    for field in 0..8 {
        let budget = Budget::default();
        let charge = || match field {
            0 => budget.ast(
                &normalized::Summary {
                    literals: AST_NODES - 2,
                    ..normalized::Summary::default()
                },
                &|| false,
            ),
            1 => budget.ast(
                &normalized::Summary {
                    class_items: CLASS_ITEMS,
                    ..normalized::Summary::default()
                },
                &|| false,
            ),
            2 => budget.hir(
                &Pressure {
                    reconstruction_entries: RECONSTRUCTION_ENTRIES,
                    ..Pressure::default()
                },
                &|| false,
            ),
            3 => budget.hir(
                &Pressure {
                    literal_byte_revisits: LITERAL_BYTE_REVISITS,
                    ..Pressure::default()
                },
                &|| false,
            ),
            4 => budget.hir(
                &Pressure {
                    candidate_range_slots: CANDIDATE_RANGE_SLOTS,
                    ..Pressure::default()
                },
                &|| false,
            ),
            5 => budget.nfa(NFA_DISPATCH, 0, 0, &|| false),
            6 => budget.nfa(0, NFA_HELPER_PAYLOAD, 0, &|| false),
            _ => budget.nfa(0, 0, NFA_EMPTY_TAIL, &|| false),
        };
        let failed = loop {
            match charge() {
                Ok(()) => (),
                Err(Error::Attempt(failed)) => break failed,
                Err(_) => panic!("aggregate charging must report its shared failure"),
            }
        };
        let expected = [
            "pattern-construction-ast-nodes",
            "pattern-construction-class-items",
            "pattern-construction-reconstruction",
            "pattern-construction-literal-revisits",
            "pattern-construction-range-slots",
            "pattern-construction-dispatch",
            "pattern-construction-helper-payload",
            "pattern-construction-empty-tail",
        ][field];
        assert_eq!(failed.code, expected);
        assert_eq!(budget.check(&|| false).unwrap_err().code, expected);
        assert_eq!(
            budget.check(&|| true).unwrap_err().code,
            expected,
            "preserve the previously observed first cause"
        );
        assert!(matches!(
            budget.nfa(usize::MAX, 0, 0, &|| false),
            Err(Error::Attempt(_))
        ));
    }
}

#[test]
fn local_refusals_do_not_debit_or_poison_shared_construction() {
    let budget = Budget::default();
    assert!(matches!(
        budget.ast(
            &normalized::Summary {
                literals: usize::MAX,
                ..normalized::Summary::default()
            },
            &|| false
        ),
        Err(Error::PatternLimit)
    ));
    assert!(matches!(
        budget.hir(
            &Pressure {
                reconstruction_entries: RECONSTRUCTION_ENTRIES + 1,
                ..Pressure::default()
            },
            &|| false
        ),
        Err(Error::PatternLimit)
    ));
    assert!(matches!(
        budget.nfa(NFA_DISPATCH + 1, 0, 0, &|| false),
        Err(Error::PatternLimit)
    ));
    budget.check(&|| false).unwrap();
    budget
        .ast(&normalized::Summary::default(), &|| false)
        .unwrap();
}

#[test]
fn earlier_sdk_failure_and_cancellation_precede_a_local_profile_refusal() {
    let budget = Budget::default();
    let result = Attempt::run(0, usize::MAX, &|| false, |control| {
        assert!(matches!(
            budget.nfa(usize::MAX, 0, 0, control),
            Err(Error::Attempt(Failure {
                reason: FailureReason::LimitExceeded,
                ..
            }))
        ));
    });
    assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
    assert!(matches!(
        budget.nfa(usize::MAX, 0, 0, &|| true),
        Err(Error::Attempt(Failure {
            reason: FailureReason::Cancelled,
            ..
        }))
    ));
    assert!(budget.failure.get().is_none());
}

#[test]
fn aggregate_arithmetic_is_checked_and_never_uses_nfa_saturation() {
    let budget = Budget::default();
    budget.used.set(Counts {
        dispatch: usize::MAX,
        ..Counts::default()
    });
    assert!(matches!(
        budget.nfa(1, 0, 0, &|| false),
        Err(Error::Attempt(_))
    ));
    assert_eq!(
        budget.check(&|| false).unwrap_err().code,
        "pattern-construction-dispatch"
    );
}
