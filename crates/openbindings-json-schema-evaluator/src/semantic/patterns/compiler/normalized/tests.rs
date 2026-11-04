use super::*;
use crate::pattern::{self, Reading};
use openbindings_internal_schema::preparation::Attempt;
use std::sync::atomic::{AtomicUsize, Ordering};

fn failure(error: Error) -> Failure {
    match error {
        Error::Attempt(failure) => failure,
        other => panic!("expected preparation failure: {other:?}"),
    }
}

#[test]
fn unchanged_reader_inventory_stays_inside_the_guard() {
    let mut linear = 0;
    for line in include_str!("tests/reader-inputs.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let source = row["pattern"].as_str().unwrap();
        if let Ok(Reading::Linear(reading)) = pattern::read(source, 32, 1024 * 1024) {
            inspect(&reading.form, usize::MAX, &|| false)
                .unwrap_or_else(|error| panic!("inventory {}: {error:?}", row["id"]));
            linear += 1;
        }
    }
    // Receipt identity for this fixed input, not a public compatibility count.
    assert_eq!(linear, 1142);
}

#[test]
fn independent_exclusions_and_boundaries() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("tests/independent-controls.json")).unwrap();
    for row in cases["cases"].as_array().unwrap() {
        let maximum = row["maximum_depth"].as_u64().unwrap_or(1000) as usize;
        let result = inspect(row["form"].as_str().unwrap(), maximum, &|| false);
        assert_eq!(
            result.is_ok(),
            row["accepted"].as_bool().unwrap(),
            "{}: {result:?}",
            row["id"]
        );
    }
}

#[test]
fn boundary_group_and_flat_class_summary() {
    let summary = inspect(r"((?-u:\b))[0-9A-Z\x{E000}]a{12,34}?", 2, &|| false).unwrap();
    assert_eq!(
        summary,
        Summary {
            groups: 2,
            maximum_group_depth: 2,
            flags: 2,
            assertions: 1,
            classes: 1,
            class_items: 3,
            class_endpoints: 5,
            class_ranges: 2,
            literals: 1,
            repetitions: 1,
            numeric_digits: 4,
            alternatives: 0,
        }
    );
    assert!(matches!(
        inspect(r"(?-u:\b)", 0, &|| false),
        Err(Error::Refused(Refusal::Depth { offset: 0 }))
    ));
    assert!(matches!(
        inspect("[z-a]", 10, &|| false),
        Err(Error::Refused(Refusal::Grammar { .. }))
    ));
    assert_eq!(
        std::mem::size_of::<Summary>(),
        12 * std::mem::size_of::<usize>()
    );
}

#[test]
fn exact_shared_work_and_one_below() {
    for bytes in [0, 1, 255, 256, 257, 1024] {
        let form = "a".repeat(bytes);
        // Precheck + prepaid scan + each 256-byte checkpoint + completion.
        let required = 8 * bytes + 3 + bytes / 256;
        let result = Attempt::run(required, 0, &|| false, |attempt| inspect(&form, 0, attempt));
        assert!(result.unwrap().is_ok(), "{bytes}");
        let result = Attempt::run(required - 1, 0, &|| false, |attempt| {
            inspect(&form, 0, attempt)
        });
        assert_eq!(result.unwrap_err().code, "preparation-work-units");
    }
}

#[test]
fn cancellation_is_checked_inside_each_long_loop() {
    let class = format!(
        "[{}]",
        (0..1000)
            .map(|n| format!(r"\x{{{n:X}}}"))
            .collect::<String>()
    );
    for form in [
        format!("a{{{}}}", "0".repeat(4096)),
        format!(r"\x{{{}1}}", "0".repeat(4096)),
        class,
    ] {
        let checks = AtomicUsize::new(0);
        let control = || checks.fetch_add(1, Ordering::Relaxed) >= 2;
        let error = failure(inspect(&form, 100, &control).unwrap_err());
        assert_eq!(error.reason, FailureReason::Cancelled);
        assert_eq!(checks.load(Ordering::Relaxed), 3);
    }
    assert_eq!(
        failure(inspect("", 0, &|| true).unwrap_err()).reason,
        FailureReason::Cancelled
    );
}

#[test]
fn arithmetic_refusal_is_sticky_and_preserves_an_existing_cause() {
    assert_eq!(
        failure(reserve_work(usize::MAX, &|| true).unwrap_err()).reason,
        FailureReason::Cancelled
    );
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        let error = failure(reserve_work(usize::MAX, attempt).unwrap_err());
        assert_eq!(error.code, "preparation-work-units");
        assert!(attempt.charge_work(0).is_err());
    });
    assert_eq!(result.unwrap_err().code, "preparation-work-units");
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert_eq!(
            failure(reserve_work(usize::MAX, attempt).unwrap_err()).code,
            "preparation-allocation-bytes"
        );
        let mut maximum = usize::MAX;
        assert_eq!(
            failure(add(&mut maximum, 1, attempt).unwrap_err()).code,
            "preparation-allocation-bytes"
        );
    });
    assert_eq!(result.unwrap_err().code, "preparation-allocation-bytes");
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        let mut maximum = usize::MAX;
        assert_eq!(
            failure(add(&mut maximum, 1, attempt).unwrap_err()).code,
            "preparation-work-units"
        );
        assert!(attempt.charge_work(0).is_err());
    });
    assert_eq!(result.unwrap_err().code, "preparation-work-units");
}
