//! Peak decimal selection, with independent Original decisions at diagnostic limits.
use super::*;

fn prepared(schema: &str) -> (DefaultValidator, DefaultValidator) {
    owners(
        schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    )
}

fn diagnostic(schema: &str) -> DefaultValidator {
    let resources = ExternalSchemas::default()
        .with(
            "https://qualification.invalid/root",
            JsonValue::parse(schema).unwrap(),
        )
        .unwrap();
    prepare_document(
        &document(r#"{"$ref":"https://qualification.invalid/root"}"#),
        &resources,
        &Original(Limits {
            evaluation_sdk_reservation_bytes: 2 * 1024 * 1024 * 1024,
            ..Limits::default()
        }),
        &PreparationLimits::default(),
    )
    .unwrap()
}

fn verdict(outcome: &ValueOutcome) -> Option<bool> {
    match outcome {
        ValueOutcome::Satisfies => Some(true),
        ValueOutcome::Fails { .. } => Some(false),
        ValueOutcome::NoVerdict { .. } => None,
        ValueOutcome::Undefined { .. } => panic!("qualification schema must be defined"),
    }
}

fn check(
    selected: &DefaultValidator,
    original: &DefaultValidator,
    diagnostic: &DefaultValidator,
    value: &JsonValue,
    enters: bool,
    expected: bool,
) {
    compiled::entries::take();
    let actual = selected.evaluate(value, Detail::Outcome, &HEALTHY);
    assert_eq!(compiled::entries::take(), [0, 0, usize::from(enters)]);
    let old = original.evaluate(value, Detail::Outcome, &HEALTHY);
    // A previous verdict cannot be lost; refusals are not disagreements.
    if let Some(old) = verdict(&old) {
        assert_eq!(verdict(&actual), Some(old));
    }
    let independent = diagnostic.evaluate(value, Detail::Outcome, &HEALTHY);
    assert_eq!(verdict(&independent), Some(expected));
    if enters {
        assert_eq!(verdict(&actual), Some(expected));
    }
    // Explain still uses Original, including its original resource policy.
    assert_eq!(
        serialized(selected.evaluate(value, Detail::Explain, &HEALTHY)),
        serialized(original.evaluate(value, Detail::Explain, &HEALTHY))
    );
}

fn array(token: &str, size: usize) -> JsonValue {
    let count = (size - 1) / (token.len() + 1);
    let mut text = format!("[{}{}]", format!("{token},").repeat(count - 1), token);
    text.insert_str(text.len() - 1, &" ".repeat(size - text.len()));
    assert_eq!(text.len(), size);
    JsonValue::parse(text).unwrap()
}

#[test]
fn money_records_cross_the_old_cliff() {
    let schema = r#"{"type":"array","items":{"type":"object","required":["sku","price","tax","discount","status"],"properties":{"sku":{"type":"string"},"price":{"type":"number","minimum":0,"maximum":1000000},"tax":{"type":"number","minimum":0,"maximum":1000000},"discount":{"type":"number","minimum":0,"maximum":1000000},"status":{"enum":["open","paid","void"]}}}}"#;
    let (selected, original) = prepared(schema);
    let independent = diagnostic(schema);
    for size in [25_000, 100_000, 16 * 1024 * 1024] {
        // Only the largest record set needs longer SKUs to fit the parser's
        // one-million-node limit. Small cases keep the measured compact shape.
        let sku = if size > 100_000 {
            "x".repeat(256)
        } else {
            "sku-0".into()
        };
        let item =
            format!(r#"{{"sku":"{sku}","price":19.99,"tax":1.65,"discount":0.5,"status":"paid"}}"#);
        check(
            &selected,
            &original,
            &independent,
            &array(&item, size),
            true,
            true,
        );
    }
}

#[test]
fn decimal_spellings_and_all_four_bounds_agree_above_the_old_estimate() {
    let tokens = [
        "1.0".to_owned(),
        "1e3".into(),
        "-0".into(),
        format!("0.{}1", "0".repeat(1024)),
        format!("0.{}", "1".repeat(1024)),
        "1e9223372036854775808".into(),
        "1e-9223372036854775809".into(),
        "-1e9223372036854775808".into(),
        "-1e-9223372036854775809".into(),
    ];
    for token in tokens {
        for keyword in ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"] {
            // An equal bound checks strictness and both operand representations.
            let schema = format!(r#"{{"items":{{"{keyword}":{token}}}}}"#);
            let (selected, original) = prepared(&schema);
            let independent = diagnostic(&schema);
            let value =
                JsonValue::parse(format!("[{token}{}]", " ".repeat(4 * 1024 * 1024))).unwrap();
            check(
                &selected,
                &original,
                &independent,
                &value,
                true,
                !keyword.starts_with("exclusive"),
            );
        }
    }
}

#[test]
fn hostile_decisions_have_an_independent_original_witness() {
    for (token, schema) in [
        (
            format!("0.{}", "1".repeat(1024 * 1024 - 2)),
            r#"{"items":{"minimum":0,"maximum":1}}"#,
        ),
        (
            format!("1e{}", "9".repeat(1024 * 1024 - 2)),
            r#"{"items":{"minimum":1}}"#,
        ),
        (
            format!("1e-{}", "9".repeat(1024 * 1024 - 3)),
            r#"{"items":{"minimum":0,"maximum":1}}"#,
        ),
    ] {
        let (selected, original) = prepared(schema);
        let independent = diagnostic(schema);
        let value = array(&token, 16 * 1024 * 1024);
        assert!(
            matches!(original.evaluate(&value, Detail::Outcome, &HEALTHY), ValueOutcome::NoVerdict { detail } if detail.code == "semantic-analysis-memory-limit")
        );
        check(&selected, &original, &independent, &value, true, true);
    }
}

#[test]
fn peak_boundary_includes_the_longest_schema_bound() {
    for bound in ["1".to_owned(), format!("1e{}", "9".repeat(1024))] {
        let schema = format!(r#"{{"items":{{"maximum":{bound}}}}}"#);
        let (selected, original) = prepared(&schema);
        let independent = diagnostic(&schema);
        let last = (48 * 1024 * 1024 - 512) / 4 - bound.len();
        for size in [last, last + 1, 16 * 1024 * 1024 - 2] {
            let token = format!("1e-{}", "9".repeat(size - 3));
            let value = JsonValue::parse(format!("[{token}]")).unwrap();
            check(
                &selected,
                &original,
                &independent,
                &value,
                size <= last,
                true,
            );
        }
    }
}

#[test]
fn decimal_scan_checks_control_and_small_values_do_not_scan() {
    struct Count {
        calls: AtomicUsize,
        cancel_at: usize,
    }
    impl WorkControl for Count {
        fn is_cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed) >= self.cancel_at
        }
    }
    let (selected, _) = prepared(r#"{"items":{"minimum":0,"maximum":1000000}}"#);
    for (size, cancel_at) in [(10_000, usize::MAX), (4 * 1024 * 1024, 20)] {
        let value = array("19.99", size);
        let control = Count {
            calls: AtomicUsize::new(0),
            cancel_at,
        };
        compiled::entries::take();
        let result = selected.compiled.outcome(&value, &control);
        if cancel_at == usize::MAX {
            assert!(matches!(result, Some(ValueOutcome::Satisfies)));
            assert_eq!(control.calls.load(Ordering::Relaxed), 2);
            assert_eq!(compiled::entries::take(), [0, 0, 1]);
        } else {
            assert!(
                matches!(result, Some(ValueOutcome::NoVerdict { detail }) if detail.reason == NoVerdictReason::Cancelled)
            );
            assert_eq!(compiled::entries::take(), [0, 0, 0]);
        }
    }
}
