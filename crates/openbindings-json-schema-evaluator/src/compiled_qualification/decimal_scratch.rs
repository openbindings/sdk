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
    let resources = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://qualification.invalid/root").into(),
        document: JsonValue::parse(schema).unwrap(),
    }])
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
) {
    compiled::entries::take();
    let actual = selected.evaluate(value, Detail::Outcome, &HEALTHY);
    assert_eq!(compiled::entries::take()[2], usize::from(enters));
    let old = original.evaluate(value, Detail::Outcome, &HEALTHY);
    // A previous verdict cannot be lost; refusals are not disagreements.
    if let Some(old) = verdict(&old) {
        assert_eq!(verdict(&actual), Some(old));
    }
    let independent = diagnostic.evaluate(value, Detail::Outcome, &HEALTHY);
    assert_eq!(verdict(&independent), Some(true));
    if enters {
        assert_eq!(verdict(&actual), Some(true));
    }
    // Explain still uses Original, including its original resource policy.
    assert_eq!(
        serialized(selected.evaluate(value, Detail::Explain, &HEALTHY)),
        serialized(original.evaluate(value, Detail::Explain, &HEALTHY))
    );
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
            check(&selected, &original, &independent, &value, size <= last);
        }
    }
}
