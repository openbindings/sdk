//! The forms the evaluator keeps for a contract's patterns are bounded
//! together. A form spells every set out, so a short pattern can have a long
//! form (`\S` takes about 200 bytes): preparation counts each form as it is
//! written, refuses at the first that passes `Limits::total_pattern_bytes`,
//! and keeps one copy of each distinct `pattern` form.
use super::measured;
use openbindings::{
    Conformant, Document, JsonValue, NoVerdict, PreparedValueContract, ValueOutcome,
};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::json;

fn conformant(input: &str) -> Conformant<Document> {
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{input}}}}}}}"#);
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant fixture")
}
fn prepare(
    proof: &Conformant<Document>,
    limits: Limits,
) -> Result<PreparedValueContract, NoVerdict> {
    proof
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::with_limits(limits))
}
// Properties each holding one of `patterns`.
fn properties(patterns: impl Iterator<Item = String>) -> String {
    let properties: Vec<String> = patterns
        .enumerate()
        .map(|(i, pattern)| format!(r#""p{i}":{{"pattern":"{pattern}"}}"#))
        .collect();
    format!(
        r#"{{"type":"object","properties":{{{}}}}}"#,
        properties.join(",")
    )
}
// A pattern of 5,000 `\S` (written as JSON), whose form is about a megabyte,
// distinguished by `i`.
fn spelled(i: usize) -> String {
    format!(r"{}z{i}", r"\\S".repeat(5_000))
}
// The bytes of the form the evaluator keeps for `pattern`: the least total
// limit within which a contract holding only it prepares.
fn form_bytes(pattern: &str) -> usize {
    let proof = conformant(&properties(std::iter::once(pattern.to_owned())));
    let (mut refused, mut admitted) = (0, Limits::default().total_pattern_bytes);
    assert!(prepare(&proof, Limits::default()).is_ok());
    while admitted - refused > 1 {
        let total = (refused + admitted) / 2;
        let limits = Limits {
            total_pattern_bytes: total,
            ..Limits::default()
        };
        if prepare(&proof, limits).is_ok() {
            admitted = total;
        } else {
            refused = total;
        }
    }
    admitted
}

pub fn run() {
    let limit = Limits::default().total_pattern_bytes;
    let form = form_bytes(&spelled(0));
    // Each form is far longer than its 10,002-byte pattern but within the
    // per-pattern limit, so only the total refuses.
    assert!(
        form > 900_000 && form <= Limits::default().pattern_bytes,
        "{form}"
    );
    let within = limit / form;
    assert!(within >= 2, "{form}");
    // Beyond what preparing the same document without patterns takes, the
    // forms the total admits and one form being written, whose buffer and
    // its shared copy can hold three times the per-pattern limit.
    let allowance = limit + 3 * Limits::default().pattern_bytes;
    let names = |count: usize| {
        let names: Vec<String> = (0..count)
            .map(|i| format!(r#""{}":true"#, spelled(i)))
            .collect();
        format!(
            r#"{{"type":"object","patternProperties":{{{}}}}}"#,
            names.join(",")
        )
    };
    let cases: Vec<(&str, String, &str)> = vec![
        // 3.8 MB of patterns whose forms take 380 MB.
        (
            "many-patterns",
            properties((0..380).map(spelled)),
            "pattern-total-byte-limit",
        ),
        (
            "within-total",
            properties((0..within).map(spelled)),
            "satisfies",
        ),
        (
            "past-total",
            properties((0..=within).map(spelled)),
            "pattern-total-byte-limit",
        ),
        // One pattern repeated keeps one form, counted once.
        (
            "repeated-pattern",
            properties((0..380).map(|_| spelled(0))),
            "satisfies",
        ),
        // The bundle keeps each name's form, so every name counts.
        (
            "names-past-total",
            names(within + 1),
            "pattern-total-byte-limit",
        ),
    ];
    let value = JsonValue::parse("{}").unwrap();
    for (case, input, expected) in cases {
        let proof = conformant(&input);
        // The control holds the same text as annotations and property names.
        let control = conformant(
            &input
                .replace(r#""pattern""#, r#""description""#)
                .replace(r#""patternProperties""#, r#""properties""#),
        );
        let (prepared, control_bytes, _) =
            measured(|| prepare(&control, Limits::default()).is_ok());
        assert!(prepared, "{case} control");
        let ceiling = control_bytes + allowance;
        for repetition in 0..3 {
            let (outcome, bytes, nanos) = measured(|| match prepare(&proof, Limits::default()) {
                Ok(contract) => match contract.validate(&value) {
                    ValueOutcome::Satisfies => "satisfies".to_owned(),
                    other => format!("{other:?}"),
                },
                Err(detail) => detail.code.to_string(),
            });
            assert_eq!(outcome, expected, "{case}");
            println!(
                "{}",
                json!({"phase":"pattern-forms","case":case,"source_bytes":input.len(),
                "form_bytes":form,"repetition":repetition,"peak_bytes":bytes,"nanos":nanos,
                "outcome":outcome,"control_peak_bytes":control_bytes,"ceiling_bytes":ceiling})
            );
            assert!(
                bytes <= ceiling,
                "pattern forms: {case}: {bytes} exceeds {ceiling} bytes"
            );
        }
    }
}
