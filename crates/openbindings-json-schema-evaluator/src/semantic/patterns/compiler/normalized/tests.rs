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
        assert_eq!(error.code, "preparation-sdk-work-units");
        assert!(attempt.charge_work(0).is_err());
    });
    assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert_eq!(
            failure(reserve_work(usize::MAX, attempt).unwrap_err()).code,
            "preparation-sdk-reservation-bytes"
        );
        let mut maximum = usize::MAX;
        assert_eq!(
            failure(add(&mut maximum, 1, attempt).unwrap_err()).code,
            "preparation-sdk-reservation-bytes"
        );
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        let mut maximum = usize::MAX;
        assert_eq!(
            failure(add(&mut maximum, 1, attempt).unwrap_err()).code,
            "preparation-sdk-work-units"
        );
        assert!(attempt.charge_work(0).is_err());
    });
    assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
}
