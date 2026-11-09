use super::*;
use openbindings_internal_schema::{FailureReason, preparation::Attempt};
use regex_syntax::hir::{Capture, Repetition};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
};

thread_local! {
    static BUILDS: Cell<usize> = const { Cell::new(0) };
    static WRAPS: Cell<usize> = const { Cell::new(0) };
}
pub(super) fn entered_backend() {
    BUILDS.with(|n| n.set(n.get() + 1));
}
pub(super) fn entered_wrapper() {
    WRAPS.with(|n| n.set(n.get() + 1));
}
fn reset() {
    BUILDS.with(|n| n.set(0));
    WRAPS.with(|n| n.set(0));
}
pub(super) fn counts() -> (usize, usize) {
    (BUILDS.with(Cell::get), WRAPS.with(Cell::get))
}
fn parse(form: &str) -> Hir {
    regex_syntax::Parser::new().parse(form).unwrap()
}
fn failure(error: Error) -> Failure {
    match error {
        Error::Attempt(failure) => failure,
        other => panic!("{other:?}"),
    }
}

#[test]
fn synthetic_logical_boundaries_do_not_claim_source_or_physical_stack_support() {
    for (captures, optional, expected) in [(255, false, 511), (254, true, 512), (256, false, 513)] {
        let mut hir = Hir::literal(b"a".to_vec());
        if optional {
            hir = Hir::repetition(Repetition {
                min: 0,
                max: Some(1),
                greedy: true,
                sub: Box::new(hir),
            });
        }
        for index in 1..=captures {
            hir = Hir::capture(Capture {
                index,
                name: None,
                sub: Box::new(hir),
            });
        }
        reset();
        if expected > MAXIMUM_PATH {
            assert!(matches!(
                prepare(&hir, 1 << 20, &|| false),
                Err(Error::ConstructionLimit)
            ));
        }
    }
}

#[test]
fn one_forward_capture_free_program_has_independent_search_caches() {
    let control = Arc::new(AtomicUsize::new(0));
    let weak = Arc::downgrade(&control);
    let hir = parse(r"^(ab|é😀){1,2}$");
    reset();
    let vm = prepare(&hir, 1 << 20, &|| {
        control.fetch_add(1, Relaxed);
        false
    })
    .unwrap();
    drop(hir);
    drop(control);
    assert!(weak.upgrade().is_none());
    std::thread::scope(|scope| {
        for (text, expected) in [
            ("ab", true),
            ("é😀ab", true),
            ("xab", false),
            ("ab\n", false),
            ("", false),
        ] {
            let vm = &vm;
            scope.spawn(move || {
                let mut cache = vm.create_cache();
                assert_eq!(vm.is_match(&mut cache, text), expected);
            });
        }
    });
}

#[test]
fn limits_are_local_and_never_retry_the_backend() {
    let hir = parse("[a-z]{64}");
    reset();
    assert!(matches!(prepare(&hir, 0, &|| false), Err(Error::Size)));
    reset();
    let result = Attempt::run(usize::MAX, usize::MAX, &|| false, |attempt| {
        let refused = prepare(&hir, 1, attempt);
        assert!(matches!(refused, Err(Error::Size)));
        attempt.check().unwrap();
        refused
    });
    assert!(matches!(result.unwrap(), Err(Error::Size)));
    reset();
    let vm = prepare(&hir, 1 << 20, &|| false).unwrap();
    assert!(vm.is_match(&mut vm.create_cache(), &"a".repeat(64)));
}

#[test]
fn failed_builds_spend_shared_admission_and_exhaustion_stops_before_entry() {
    let hir = parse("a{65534}");
    let budget = profile::Budget::default();
    reset();
    for _ in 0..16 {
        assert!(matches!(
            super::prepare(&hir, 1, &budget, &|| false),
            Err(Error::Size)
        ));
    }
    let failed = failure(super::prepare(&hir, 1, &budget, &|| false).unwrap_err());
    assert_eq!(failed.code, "pattern-construction-dispatch");
    assert_eq!(budget.check(&|| false).unwrap_err().code, failed.code);
}

#[test]
fn a_local_actual_hir_profile_limit_never_enters_the_backend() {
    let budget = profile::Budget::default();
    reset();
    assert!(matches!(
        super::prepare(&parse("a{65535}"), 1 << 20, &budget, &|| false),
        Err(Error::ConstructionLimit)
    ));
    budget.check(&|| false).unwrap();
    super::prepare(&parse("a"), 1 << 20, &budget, &|| false).unwrap();
}

#[test]
fn cancellation_at_each_boundary_prevents_publication_and_retry_is_fresh() {
    let hir = parse("ab|cd");
    let observations = AtomicUsize::new(0);
    let count = || {
        observations.fetch_add(1, Relaxed);
        false
    };
    prepare(&hir, 1 << 20, &count).unwrap();
    let checkpoints = observations.load(Relaxed);
    for stop in 0..checkpoints {
        observations.store(0, Relaxed);
        reset();
        let cancel = || observations.fetch_add(1, Relaxed) >= stop;
        assert_eq!(
            failure(prepare(&hir, 1 << 20, &cancel).unwrap_err()).reason,
            FailureReason::Cancelled
        );
        reset();
        prepare(&hir, 1 << 20, &|| false).unwrap();
    }
    reset();
    assert_eq!(
        failure(prepare(&hir, 1, &|| counts().0 > 0).unwrap_err()).reason,
        FailureReason::Cancelled
    );
}

#[test]
fn prior_shared_failure_wins_over_local_limit_and_cannot_be_hidden() {
    let hir = parse("ab|cd");
    reset();
    let result = Attempt::run(100, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert_eq!(
            failure(prepare(&hir, 0, attempt).unwrap_err()).code,
            "preparation-sdk-reservation-bytes"
        );
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
    let result = Attempt::run(0, usize::MAX, &|| false, |attempt| {
        assert!(matches!(
            prepare(&hir, 1 << 20, attempt),
            Err(Error::Attempt(_))
        ));
    });
    assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
}

fn prepare(hir: &Hir, bytes: usize, control: &dyn Control) -> Result<PikeVM, Error> {
    super::prepare(hir, bytes, &profile::Budget::default(), control)
}
