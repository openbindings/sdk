use super::*;
use crate::pattern::{self, Reading};
use openbindings_internal_schema::preparation::Attempt;
use regex_automata::nfa::thompson::{self, pikevm::PikeVM};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
};

thread_local! {static ENTRIES:Cell<usize>=const{Cell::new(0)};}
pub(super) fn entered_translator() {
    ENTRIES.with(|n| n.set(n.get() + 1));
}
fn entries() -> usize {
    ENTRIES.with(Cell::get)
}
fn reset() {
    ENTRIES.with(|n| n.set(0));
}
fn controls() -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(include_str!("tests/independent.json")).unwrap()["cases"].as_array().unwrap().clone()
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
    checks: AtomicUsize,
}
impl Control for Recorder {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn check(&self) -> Result<(), Failure> {
        self.checks.fetch_add(1, Relaxed);
        self.charge_work(1)
    }
    fn charge_work(&self, n: usize) -> Result<(), Failure> {
        self.work.fetch_add(n, Relaxed);
        Ok(())
    }
    fn charge_allocation(&self, n: usize) -> Result<(), Failure> {
        self.check()?;
        self.allocation.fetch_add(n, Relaxed);
        Ok(())
    }
}
fn vm(hir: &Hir) -> PikeVM {
    let nfa = thompson::NFA::compiler()
        .configure(thompson::Config::new().which_captures(thompson::WhichCaptures::None))
        .build_from_hir(hir)
        .unwrap();
    PikeVM::builder().build_from_nfa(nfa).unwrap()
}

#[test]
fn independently_fixed_logical_profile_boundaries() {
    for row in controls().into_iter().filter(|r| r["kind"] == "profile") {
        reset();
        let result = prepare(row["form"].as_str().unwrap(), &|| false);
        if row["expect"] == "profile-refusal-before-translator" {
            assert!(
                matches!(result, Err(Error::TranslationNesting)),
                "{} {result:?}",
                row["id"]
            );
            assert_eq!(entries(), 0);
        } else {
            drop(result.unwrap());
            assert_eq!(entries(), 1, "{}", row["id"]);
        }
    }
}
#[test]
fn normalized_depth_accounts_for_reader_synthesized_groups() {
    for row in controls()
        .into_iter()
        .filter(|r| r["kind"] == "reader-representation" || r["kind"] == "reader-contract")
    {
        let limit = match row.get("maximum_source_depth") {
            Some(v) if v == "usize::MAX" => usize::MAX,
            Some(v) => v.as_u64().unwrap() as usize,
            None => 32,
        };
        reset();
        let reading = pattern::read(row["source"].as_str().unwrap(), limit, 1024 * 1024);
        let expectation = row["expect"].as_str().unwrap();
        if expectation.starts_with("Reader Linear") {
            let Ok(Reading::Linear(reading)) = reading else {
                panic!("{}: {reading:?} expected {expectation}", row["id"]);
            };
            drop(prepare(&reading.form, &|| false).unwrap());
            assert_eq!(entries(), 1, "{}", row["id"]);
        } else if expectation.starts_with("Reader Unsupported(PropertyEscape)") {
            assert!(
                matches!(
                    reading,
                    Ok(Reading::Unsupported(pattern::Unsupported::PropertyEscape))
                ),
                "{}: {reading:?}",
                row["id"]
            );
            assert_eq!(entries(), 0);
        } else {
            assert!(
                expectation.starts_with("Reader Depth") || expectation.starts_with("Reader depth"),
                "unknown control {expectation}"
            );
            assert!(
                matches!(reading, Err(pattern::Refusal::Depth)),
                "{}: {reading:?}",
                row["id"]
            );
            assert_eq!(entries(), 0);
        }
    }
}
#[test]
fn configured_semantics_match_independent_values() {
    for row in controls()
        .into_iter()
        .filter(|r| r["kind"] == "configuration")
    {
        let hir = prepare(row["form"].as_str().unwrap(), &|| false).unwrap();
        let vm = vm(&hir);
        let mut cache = vm.create_cache();
        for (value, expected) in row["values"]
            .as_array()
            .unwrap()
            .iter()
            .zip(row["expected_exists"].as_array().unwrap())
        {
            assert_eq!(
                vm.is_match(&mut cache, value.as_str().unwrap()),
                expected.as_bool().unwrap(),
                "{}",
                row["id"]
            );
        }
    }
}
#[test]
fn excluded_backend_grammar_never_enters_translator() {
    for row in controls()
        .into_iter()
        .filter(|r| r["kind"] == "closed-grammar")
    {
        reset();
        assert!(
            matches!(
                prepare(row["form"].as_str().unwrap(), &|| false),
                Err(Error::Representation)
            ),
            "{}",
            row["id"]
        );
        assert_eq!(entries(), 0);
    }
}
#[test]
fn exact_and_one_below_cumulative_reservations() {
    for form in ["", "a0000|b0000", "[a-z]", "((a|b)*)"] {
        let recorder = Recorder::default();
        drop(prepare(form, &recorder).unwrap());
        let work = recorder.work.load(Relaxed);
        let allocation = recorder.allocation.load(Relaxed);
        reset();
        drop(
            Attempt::run(work, allocation, &|| false, |a| prepare(form, a))
                .unwrap()
                .unwrap(),
        );
        assert_eq!(entries(), 1);
        reset();
        let result = Attempt::run(work - 1, allocation, &|| false, |a| prepare(form, a));
        assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
        assert_eq!(entries(), 1);
        reset();
        let result = Attempt::run(work, allocation - 1, &|| false, |a| prepare(form, a));
        assert_eq!(
            result.unwrap_err().code,
            "preparation-sdk-reservation-bytes"
        );
        assert_eq!(entries(), 0);
    }
}
#[test]
fn refused_translation_precharges_cannot_enter_backend() {
    let form = "a0000|b0000";
    let recorder = Recorder::default();
    recorder.charge_work(entry_work().unwrap()).unwrap();
    recorder.check().unwrap();
    let parsed = ast::prepare(form, 33, AST_REPRESENTATION_DEPTH, &recorder).unwrap();
    let owned = inventory::inspect(parsed, &recorder).unwrap();
    let before = recorder.work.load(Relaxed);
    let hir_work = owned.work + form.len();
    drop(owned);
    reset();
    let result = Attempt::run(before + hir_work - 1, usize::MAX, &|| false, |a| {
        prepare(form, a)
    });
    assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
    assert_eq!(entries(), 0);
}
#[test]
fn cancellation_and_prior_failure_are_sticky_and_retry_is_fresh() {
    let form = "a0000|b0000";
    reset();
    assert_eq!(
        failure(prepare(form, &|| true).unwrap_err()).reason,
        FailureReason::Cancelled
    );
    assert_eq!(entries(), 0);
    reset();
    let result = Attempt::run(usize::MAX, usize::MAX, &|| entries() > 0, |a| {
        prepare(form, a)
    });
    assert_eq!(result.unwrap_err().reason, FailureReason::Cancelled);
    assert_eq!(entries(), 1);
    let observations = AtomicUsize::new(0);
    drop(
        Attempt::run(
            usize::MAX,
            usize::MAX,
            &|| {
                observations.fetch_add(1, Relaxed);
                false
            },
            |a| prepare(form, a),
        )
        .unwrap()
        .unwrap(),
    );
    let checkpoints = observations.load(Relaxed);
    for stop in 0..checkpoints {
        observations.store(0, Relaxed);
        reset();
        let cancel = || observations.fetch_add(1, Relaxed) >= stop;
        let failed =
            Attempt::run(usize::MAX, usize::MAX, &cancel, |a| prepare(form, a)).unwrap_err();
        assert_eq!(
            failed.reason,
            FailureReason::Cancelled,
            "checkpoint {stop}/{checkpoints}"
        );
        reset();
        drop(prepare(form, &|| false).unwrap());
        assert_eq!(entries(), 1);
    }
    let result = Attempt::run(100, 0, &|| false, |a| {
        a.charge_allocation(1).unwrap_err();
        assert_eq!(work_overflow(a).code, "preparation-sdk-reservation-bytes");
        assert_eq!(
            allocation_overflow(a).code,
            "preparation-sdk-reservation-bytes"
        );
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
fn nested_attempts_share_reservations_and_cannot_hide_a_lower_refusal() {
    let form = "a0000|b0000";
    let recorder = Recorder::default();
    drop(prepare(form, &recorder).unwrap());
    let work = recorder.work.load(Relaxed);
    let allocation = recorder.allocation.load(Relaxed);
    let result = Attempt::run(usize::MAX, allocation, &|| false, |outer| {
        drop(
            Attempt::run(usize::MAX, allocation, outer, |inner| prepare(form, inner))
                .unwrap()
                .unwrap(),
        );
        assert_eq!(outer.allocated(), allocation);
    });
    result.unwrap();
    for (child_work, child_allocation, code) in [
        (work - 1, usize::MAX, "preparation-sdk-work-units"),
        (
            usize::MAX,
            allocation - 1,
            "preparation-sdk-reservation-bytes",
        ),
    ] {
        let failed = Attempt::run(usize::MAX, usize::MAX, &|| false, |outer| {
            assert!(
                Attempt::run(child_work, child_allocation, outer, |inner| prepare(
                    form, inner
                ))
                .is_err()
            );
            assert_eq!(outer.check().unwrap_err().code, code);
        })
        .unwrap_err();
        assert_eq!(failed.code, code);
        drop(prepare(form, &|| false).unwrap());
    }
}

#[test]
fn cancellation_at_the_profile_refusal_takes_precedence() {
    let row = controls()
        .into_iter()
        .find(|r| r["id"] == "hir-stage/prefix-65")
        .unwrap();
    let form = row["form"].as_str().unwrap();
    let recorder = Recorder::default();
    assert!(matches!(
        prepare(form, &recorder),
        Err(Error::TranslationNesting)
    ));
    let checks = recorder.checks.load(Relaxed);
    struct CancelAt {
        observations: AtomicUsize,
        checkpoint: usize,
    }
    impl Control for CancelAt {
        fn is_cancelled(&self) -> bool {
            false
        }
        fn check(&self) -> Result<(), Failure> {
            if self.observations.fetch_add(1, Relaxed) + 1 == self.checkpoint {
                Err(Failure::new(
                    FailureReason::Cancelled,
                    "caller-cancelled",
                    "cancelled at profile completion",
                ))
            } else {
                Ok(())
            }
        }
        fn charge_work(&self, _: usize) -> Result<(), Failure> {
            Ok(())
        }
        fn charge_allocation(&self, _: usize) -> Result<(), Failure> {
            self.check()
        }
    }
    reset();
    let cancel = CancelAt {
        observations: AtomicUsize::new(0),
        checkpoint: checks,
    };
    assert_eq!(
        failure(prepare(form, &cancel).unwrap_err()).reason,
        FailureReason::Cancelled
    );
    assert_eq!(entries(), 0);
}
#[test]
fn owns_hir_after_source_and_control_drop_and_supports_independent_callers() {
    let source = "a0000|b0000".to_owned();
    let control = Arc::new(Recorder::default());
    let weak = Arc::downgrade(&control);
    let hir = prepare(&source, control.as_ref()).unwrap();
    drop(source);
    drop(control);
    assert!(weak.upgrade().is_none());
    std::thread::scope(|scope| {
        for value in ["a0000", "b0000"] {
            let hir = &hir;
            scope.spawn(move || {
                let vm = vm(hir);
                let mut cache = vm.create_cache();
                assert!(vm.is_match(&mut cache, value));
            });
        }
    });
}
#[test]
fn retained_forms_translate_once_with_matching_hir() {
    let mut count = 0;
    for line in include_str!("../normalized/tests/reader-inputs.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let Ok(Reading::Linear(reading)) =
            pattern::read(row["pattern"].as_str().unwrap(), 32, 1024 * 1024)
        else {
            continue;
        };
        reset();
        let hir = prepare(&reading.form, &|| false).unwrap();
        assert_eq!(entries(), 1, "{}", row["id"]);
        let baseline = regex_syntax::Parser::new().parse(&reading.form).unwrap();
        assert_eq!(hir, baseline, "{}", row["id"]);
        count += 1;
    }
    assert_eq!(count, 1142);
}
