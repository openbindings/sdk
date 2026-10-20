//! Fixed-check memory does not grow with a pattern near miss's length.
//!
//! A near miss is the input on which a backtracking matcher keeps the most
//! state: a saved position for each character it may retreat over. The fixed
//! patterns run on a finite automaton instead, so a check of a near miss eight
//! times longer allocates no more, in peak bytes or in allocation count, than
//! the shorter one. Parsing, which stores the document, is outside the
//! measured regions; so is the location of a finding, whose pointer is as
//! long as any member name it passes through.
use super::{ALLOCATIONS, JsonValue, Relaxed, measured};
use openbindings::{Conformance, Document};
use openbindings_internal_schema::fixed_schema;
use serde_json::json;

const SIZES: [usize; 2] = [1 << 20, 8 << 20];

fn filled(bytes: usize, head: &str, unit: &str, tail: &str) -> String {
    let body = bytes.saturating_sub(head.len() + tail.len());
    let mut text = String::with_capacity(bytes);
    text.push_str(head);
    text.push_str(&unit.repeat(body / unit.len()));
    text.push_str(&"a".repeat(body % unit.len()));
    text.push_str(tail);
    text
}

// One measured call: its result, peak incremental bytes and allocation count.
fn counted<T>(call: impl FnOnce() -> T) -> (T, usize, usize) {
    let before = ALLOCATIONS.load(Relaxed);
    let (result, peak, _) = measured(call);
    (result, peak, ALLOCATIONS.load(Relaxed) - before)
}

pub fn run() {
    // (shape, head, repeated unit, tail, meta-schema check)
    let shapes = [
        // SemVer, the costliest pattern: dotted pre-release identifiers, and
        // one long identifier.
        (
            "semver-dotted",
            r#"{"openbindings":"1.0.0-"#,
            "a.",
            r#"a!","operations":{}}"#,
            false,
        ),
        (
            "semver-plain",
            r#"{"openbindings":"1.0.0-"#,
            "a",
            r#"!","operations":{}}"#,
            false,
        ),
        // The name grammar, on an alias.
        (
            "name",
            r#"{"openbindings":"0.2.0","operations":{"o":{"aliases":[""#,
            "a.-",
            r#"a!"]}}}"#,
            false,
        ),
        // The meta-schemas' anchor and identifier patterns, on one schema.
        ("anchor", r#"{"$anchor":""#, "a.-", r#"a!"}"#, true),
        ("identifier", r#"{"$id":""#, "a/", r#"a##"}"#, true),
    ];
    // Compile the fixed schemas and warm the matcher caches first.
    for (_, head, unit, tail, meta) in shapes {
        let value = JsonValue::parse(filled(4096, head, unit, tail)).unwrap();
        assert_eq!(fixed_schema::validity(&value, meta), Ok(false));
        fixed_schema::check(&value, meta, 4096).unwrap();
    }
    for (shape, head, unit, tail, meta) in shapes {
        let mut samples = Vec::new();
        for bytes in SIZES {
            let text = filled(bytes, head, unit, tail);
            let value = JsonValue::parse(&text).unwrap();
            let (valid, validity_peak, validity_allocations) =
                counted(|| fixed_schema::validity(&value, meta));
            assert_eq!(valid, Ok(false), "{shape}");
            let (problems, check_peak, check_allocations) =
                counted(|| fixed_schema::check(&value, meta, 4096).map(|p| p.entries.len()));
            assert!(problems.unwrap() > 0, "{shape}");
            // The complete document validation, for the record; its native
            // rules and finding locations are outside this bound.
            let complete = (!meta).then(|| {
                let document = Document::parse(&text).unwrap();
                let (validation, peak, nanos) = measured(|| document.validate().unwrap());
                assert_eq!(validation.report().conclusion, Conformance::NonConformant);
                json!({"sdk_peak_bytes":peak,"sdk_nanos":nanos})
            });
            println!(
                "{}",
                json!({"phase":"near-miss","shape":shape,"source_bytes":text.len(),
                "validity_peak_bytes":validity_peak,"validity_allocations":validity_allocations,
                "check_peak_bytes":check_peak,"check_allocations":check_allocations,
                "complete":complete})
            );
            samples.push([
                validity_peak,
                validity_allocations,
                check_peak,
                check_allocations,
            ]);
        }
        let [short, long] = [samples[0], samples[1]];
        assert!(
            long.iter().zip(short).all(|(long, short)| *long <= short),
            "near-miss check memory grows with length: {shape}: {short:?} then {long:?}"
        );
    }
}
