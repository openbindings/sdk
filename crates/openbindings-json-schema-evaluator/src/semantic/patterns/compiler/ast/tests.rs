use super::*;
use crate::pattern::{self, Reading};
use openbindings_internal_schema::preparation::Attempt;
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! {
    static PARSER_ENTRIES: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn entered_parser() {
    PARSER_ENTRIES.with(|entries| entries.set(entries.get() + 1));
}

fn entries() -> usize {
    PARSER_ENTRIES.with(Cell::get)
}

fn reset_entries() {
    PARSER_ENTRIES.with(|entries| entries.set(0));
}

fn failure(error: Error) -> Failure {
    match error {
        Error::Attempt(failure) => failure,
        other => panic!("expected attempt failure: {other:?}"),
    }
}

#[derive(Default)]
struct Recorder {
    work: AtomicUsize,
    allocation: AtomicUsize,
}

impl Control for Recorder {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn check(&self) -> Result<(), Failure> {
        self.charge_work(1)
    }
    fn charge_work(&self, units: usize) -> Result<(), Failure> {
        self.work.fetch_add(units, Ordering::Relaxed);
        Ok(())
    }
    fn charge_allocation(&self, bytes: usize) -> Result<(), Failure> {
        self.check()?;
        self.allocation.fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn owns_one_complete_tree_and_its_exact_inventory() {
    reset_entries();
    let form = r"((?-u:\b))[0-9A-Z\x{E000}]a{12,34}?";
    let parsed = prepare(form, 2, 32, &|| false).unwrap();
    assert_eq!(entries(), 1);
    assert_eq!(parsed.inventory.groups, 2);
    assert_eq!(parsed.inventory.class_items, 3);
    assert_eq!(parsed.inventory.repetitions, 1);
    assert_eq!(parsed.tree.span().end.offset, form.len());
}

#[test]
fn grammar_depth_and_backend_representation_are_local_refusals() {
    reset_entries();
    assert!(matches!(
        prepare("(?i:a)", 32, 32, &|| false),
        Err(Error::Refused(Refusal::Normalized(
            normalized::Refusal::Grammar { .. }
        )))
    ));
    assert!(matches!(
        prepare("(a)", 0, 32, &|| false),
        Err(Error::Refused(Refusal::Normalized(
            normalized::Refusal::Depth { .. }
        )))
    ));
    assert_eq!(entries(), 0);
    for (form, depth) in [("a{4294967296}", 32), ("(((a)))", 0), ("*", 32)] {
        assert!(matches!(
            prepare(form, 32, depth, &|| false),
            Err(Error::Refused(Refusal::Representation))
        ));
    }
    assert_eq!(entries(), 3);
}

#[test]
fn exact_shared_allowances_and_one_below() {
    for form in ["", "a", "a0000|b0000", "[a-z]", "((a|b)*)"] {
        let recorder = Recorder::default();
        drop(prepare(form, 32, 32, &recorder).unwrap());
        let work = recorder.work.load(Ordering::Relaxed);
        let allocation = recorder.allocation.load(Ordering::Relaxed);
        reset_entries();
        let result = Attempt::run(work, allocation, &|| false, |attempt| {
            prepare(form, 32, 32, attempt)
        });
        drop(result.unwrap().unwrap());
        assert_eq!(entries(), 1, "{form}");

        // One missing work unit refuses completion after entering the parser.
        reset_entries();
        let result = Attempt::run(work - 1, allocation, &|| false, |attempt| {
            prepare(form, 32, 32, attempt)
        });
        assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
        assert_eq!(entries(), 1, "{form}");

        reset_entries();
        let result = Attempt::run(work, allocation - 1, &|| false, |attempt| {
            prepare(form, 32, 32, attempt)
        });
        assert_eq!(
            result.unwrap_err().code,
            "preparation-sdk-reservation-bytes"
        );
        assert_eq!(entries(), 0, "{form}");
    }
}

#[test]
fn refused_precharges_never_enter_the_parser() {
    for limit in [0, 1, 100, SCHEDULE_WORK, SCHEDULE_WORK + 100] {
        reset_entries();
        let result = Attempt::run(limit, usize::MAX, &|| false, |attempt| {
            prepare("a0000|b0000", 32, 32, attempt)
        });
        assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
        assert_eq!(entries(), 0);
    }
    reset_entries();
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert_eq!(
            failure(prepare("a", 32, 32, attempt).unwrap_err()).code,
            "preparation-sdk-reservation-bytes"
        );
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
    assert_eq!(entries(), 0);
}

#[test]
fn cancellation_before_and_after_parsing_cannot_publish_ownership() {
    reset_entries();
    assert_eq!(
        failure(prepare("a", 32, 32, &|| true).unwrap_err()).reason,
        FailureReason::Cancelled
    );
    assert_eq!(entries(), 0);

    let result = Attempt::run(usize::MAX, usize::MAX, &|| entries() > 0, |attempt| {
        prepare("((a|b)*)", 32, 32, attempt)
    });
    assert_eq!(result.unwrap_err().reason, FailureReason::Cancelled);
    assert_eq!(entries(), 1);
    // A later call has independent control and owns a fresh successful AST.
    drop(prepare("((a|b)*)", 32, 32, &|| false).unwrap());
    assert_eq!(entries(), 2);
}

#[test]
fn schedule_overflow_preserves_and_poisons_the_enclosing_cause() {
    let layout = Layout::current().unwrap();
    let huge = Summary {
        literals: usize::MAX,
        ..Summary::default()
    };
    let result = Attempt::run(usize::MAX, usize::MAX, &|| false, |attempt| {
        let error = reservation(usize::MAX, &huge, layout, attempt).unwrap_err();
        assert_eq!(error.code, "preparation-sdk-work-units");
        assert!(attempt.charge_work(0).is_err());
    });
    assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");

    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        let error = reservation(usize::MAX, &huge, layout, attempt).unwrap_err();
        assert_eq!(error.code, "preparation-sdk-reservation-bytes");
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );

    let result = Attempt::run(100, usize::MAX, &|| false, |attempt| {
        // This count overflows allocation terms while its word-work sum fits.
        // It challenges arithmetic directly without manufacturing a giant input.
        let items = Summary {
            class_items: usize::MAX / 1024,
            ..Summary::default()
        };
        assert_eq!(
            reservation(0, &items, layout, attempt).unwrap_err().code,
            "preparation-sdk-reservation-bytes"
        );
        assert!(attempt.charge_allocation(0).is_err());
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
    assert_eq!(work_overflow(&|| true).reason, FailureReason::Cancelled);
    assert_eq!(
        allocation_overflow(&|| true).reason,
        FailureReason::Cancelled
    );
}

#[test]
fn target_schedules_match_the_reviewed_tiny_program() {
    let inventory = normalized::inspect("a0000|b0000", 32, &|| false).unwrap();
    for (word, expected) in [(8, 5147), (4, 5207)] {
        let layout = Layout::pinned(word).unwrap();
        let cost = reservation(11, &inventory, layout, &|| false).unwrap();
        assert_eq!(cost.work, expected);
    }
    assert!(Layout::current().is_some());
    assert!(Layout::pinned(2).is_none());
    assert!(Layout::pinned(16).is_none());
}

#[test]
fn unchanged_reader_forms_use_the_guarded_stage() {
    let mut linear = 0;
    for line in include_str!("../normalized/tests/reader-inputs.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let source = row["pattern"].as_str().unwrap();
        let Ok(Reading::Linear(reading)) = pattern::read(source, 32, 1024 * 1024) else {
            continue;
        };
        reset_entries();
        let result = prepare(&reading.form, 32, 250, &|| false);
        assert_eq!(entries(), 1, "{}: {result:?}", row["id"]);
        assert!(!matches!(result, Err(Error::Attempt(_))), "{}", row["id"]);
        linear += 1;
    }
    assert_eq!(linear, 1142);
}
