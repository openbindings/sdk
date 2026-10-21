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
    compiled();
}

// A contract's compiled patterns share its pattern memory
// (`Limits::total_pattern_memory`). Counted repetitions of a large set as
// `patternProperties` names, `^.{1,10000}N$` (about 13.2 MiB compiled each),
// once each compiled within the per-pattern size: 40 of them, under 1 KB of
// text, peaked at 1.1 GB, and 300 took over 10 s. Each needs more than its
// equal part of what the memory leaves, so preparation refuses after trying
// each within its part, one at a time, holding at most three times the memory
// divided among them (the part being compiled and its compilation's own
// working memory) beyond the same document with the names as property names.
// Four such names fit their parts: preparing them keeps no more than the
// pattern memory, and peaks within twice it.
fn compiled() {
    let memory = Limits::default().total_pattern_memory;
    let names = |count: usize| {
        let names: Vec<String> = (0..count)
            .map(|i| format!(r#""^.{{1,10000}}{i}$":true"#))
            .collect();
        format!(
            r#"{{"type":"object","patternProperties":{{{}}}}}"#,
            names.join(",")
        )
    };
    let value = JsonValue::parse(r#"{"a0":1}"#).unwrap();
    for (case, count, expected) in [
        ("counted-names-40", 40, "pattern-total-memory-limit"),
        ("counted-names-300", 300, "pattern-total-memory-limit"),
        ("counted-names-4", 4, "satisfies"),
    ] {
        let input = names(count);
        let proof = conformant(&input);
        let control = conformant(&input.replace(r#""patternProperties""#, r#""properties""#));
        let (prepared, control_bytes, _) =
            measured(|| prepare(&control, Limits::default()).is_ok());
        assert!(prepared, "{case} control");
        for repetition in 0..3 {
            let before = super::LIVE.load(std::sync::atomic::Ordering::Relaxed);
            let ((outcome, kept), bytes, nanos) =
                measured(|| match prepare(&proof, Limits::default()) {
                    Ok(contract) => {
                        let kept = super::LIVE
                            .load(std::sync::atomic::Ordering::Relaxed)
                            .saturating_sub(before);
                        let outcome = match contract.validate(&value) {
                            ValueOutcome::Satisfies => "satisfies".to_owned(),
                            other => format!("{other:?}"),
                        };
                        (outcome, kept)
                    }
                    Err(detail) => (detail.code.to_string(), 0),
                });
            assert_eq!(outcome, expected, "{case}");
            println!(
                "{}",
                json!({"phase":"compiled-patterns","case":case,"names":count,
                "source_bytes":input.len(),"repetition":repetition,"peak_bytes":bytes,
                "kept_bytes":kept,"nanos":nanos,"outcome":outcome,
                "control_peak_bytes":control_bytes,"pattern_memory":memory})
            );
            let ceiling = if outcome == "satisfies" {
                assert!(
                    kept <= control_bytes + memory,
                    "compiled patterns: {case}: keeps {kept} bytes"
                );
                control_bytes + 2 * memory
            } else {
                // Refusing takes far less than the 2 s a host may give it.
                assert!(
                    nanos < 2_000_000_000,
                    "compiled patterns: {case}: {nanos} ns"
                );
                control_bytes + 3 * (memory / count)
            };
            assert!(
                bytes <= ceiling,
                "compiled patterns: {case}: {bytes} exceeds {ceiling} bytes"
            );
        }
    }
    search_caches(memory);
}

// A search keeps its lazy DFA's cache within its pattern's equal part of the
// quarter of the pattern memory the caches have. Each of 100 small names whose
// automaton has a state for every recent run of 21 bits, `[01]*1[01]{20}zN`,
// kept about 3.3 MB of cache after searching a 64 KiB key of random bits,
// 400 MB in all; now checking that value holds no more than the pattern memory
// beyond the same document with the names as property names.
fn search_caches(memory: usize) {
    let names: Vec<String> = (0..100)
        .map(|i| format!(r#""[01]*1[01]{{20}}z{i}":true"#))
        .collect();
    let input = format!(
        r#"{{"type":"object","patternProperties":{{{}}}}}"#,
        names.join(",")
    );
    let mut state = 12_345_u64;
    let key: String = (0..65_536)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            if (state >> 33) & 1 == 1 { '1' } else { '0' }
        })
        .collect();
    let value = JsonValue::parse(format!(r#"{{"{key}":1}}"#)).unwrap();
    let proof = conformant(&input);
    let control = conformant(&input.replace(r#""patternProperties""#, r#""properties""#));
    let (decided, control_bytes, _) = measured(|| {
        let contract = prepare(&control, Limits::default()).unwrap();
        matches!(contract.validate(&value), ValueOutcome::Satisfies)
    });
    assert!(decided, "search caches control");
    for repetition in 0..3 {
        let (outcome, bytes, nanos) = measured(|| {
            let contract = prepare(&proof, Limits::default()).unwrap();
            match contract.validate(&value) {
                ValueOutcome::Satisfies => "satisfies".to_owned(),
                other => format!("{other:?}"),
            }
        });
        assert_eq!(outcome, "satisfies", "search caches");
        println!(
            "{}",
            json!({"phase":"compiled-patterns","case":"search-caches","names":100,
            "source_bytes":input.len(),"value_bytes":key.len(),"repetition":repetition,
            "peak_bytes":bytes,"nanos":nanos,"outcome":outcome,
            "control_peak_bytes":control_bytes,"pattern_memory":memory})
        );
        let ceiling = control_bytes + memory;
        assert!(
            bytes <= ceiling,
            "search caches: {bytes} exceeds {ceiling} bytes"
        );
    }
}
