//! Immutable pattern programs share one defined footprint per preparation.
//! Search caches belong to individual value calls. The retired compiler's
//! fixed 48 KiB slots, quarter-cache division and first-use compilation are
//! deliberately not expectations of this public policy test.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Map, Value, json};

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

fn prepare(
    schema: &Value,
    limits: Limits,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare(
            &DefaultEvaluator::with_limits(limits),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
}
fn memory(bytes: usize) -> Limits {
    Limits {
        total_pattern_memory: bytes,
        ..Limits::default()
    }
}
fn value(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn satisfies(
    contract: &PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>,
    text: &str,
) -> bool {
    matches!(contract.evaluate(&value(text)), ValueOutcome::Satisfies)
}
fn no_verdict(
    contract: &PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>,
    text: &str,
) -> NoVerdict {
    match contract.evaluate(&value(text)) {
        ValueOutcome::NoVerdict { detail } => detail,
        _ => panic!("{text} is decided"),
    }
}
// `count` `patternProperties` names of the form `^.{1,10000}N$`, each of which
// compiles to about 13.2 MiB, with `schema` as each name's schema.
fn counted_names(count: usize, schema: Value) -> Value {
    let names: Map<String, Value> = (0..count)
        .map(|i| (format!("^.{{1,10000}}{i}$"), schema.clone()))
        .collect();
    json!({"type": "object", "patternProperties": names})
}
// Properties `p0` and on, each a string holding the matching pattern.
fn pattern_values(patterns: &[String]) -> Value {
    let properties: Map<String, Value> = patterns
        .iter()
        .enumerate()
        .map(|(i, pattern)| (format!("p{i}"), json!({"pattern": pattern})))
        .collect();
    json!({"type": "object", "properties": properties})
}
// `count` small compiled patterns, `a0|b` and on.
fn small(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("a{i}|b")).collect()
}
fn assert_memory_refusal(refused: &PreparationFailure) {
    assert_eq!(
        refused.kind,
        PreparationFailureKind::Limit {
            resource: "pattern-total-memory-limit".into(),
        }
    );
}
// Measure a representation-policy boundary, never a semantic oracle. The same
// schema/value expectations below stay fixed independently of this measurement.
fn minimum_memory(schema: &Value) -> usize {
    let (mut below, mut enough) = (0, MIB);
    prepare(schema, memory(enough)).expect("funded representation");
    while enough - below > 1 {
        let midpoint = below + (enough - below) / 2;
        match prepare(schema, memory(midpoint)) {
            Ok(_) => enough = midpoint,
            Err(failure) => {
                assert_memory_refusal(&failure);
                below = midpoint;
            }
        }
    }
    assert_memory_refusal(&prepare(schema, memory(enough - 1)).unwrap_err());
    prepare(schema, memory(enough)).unwrap();
    enough
}

#[test]
fn the_default_pattern_memory_is_64_mib() {
    assert_eq!(Limits::default().total_pattern_memory, 64 * MIB);
}

// These unchanged large jobs exhaust the shared structural construction
// allowance before they reach the representation cap. Exhaustion aborts the
// whole preparation, equally for names and value patterns.
#[test]
fn counted_repetitions_exhaust_one_shared_construction_allowance() {
    for count in [40, 300] {
        let refused = prepare(&counted_names(count, json!(true)), Limits::default()).unwrap_err();
        assert_eq!(
            refused.kind,
            PreparationFailureKind::Limit {
                resource: "pattern-construction-helper-payload".into(),
            }
        );
    }
}

#[test]
fn large_counted_repetitions_within_the_defined_footprint_decide() {
    let alone = json!({"type": "object", "patternProperties": {"^.{1,10000}$": false}});
    // Five names no longer pay the retired compiler's per-slot cache share.
    for schema in [
        alone,
        counted_names(4, json!(false)),
        counted_names(5, json!(false)),
    ] {
        let contract = prepare(&schema, Limits::default()).unwrap();
        assert!(satisfies(&contract, r#"{"":1}"#));
        assert!(matches!(
            contract.evaluate(&value(r#"{"a0":1}"#)),
            ValueOutcome::Fails { .. }
        ));
    }
}

#[test]
fn small_patterns_do_not_take_fixed_compiler_slots_from_large_patterns() {
    for count in [0, 10, 100, 1000] {
        let schema = json!({
            "type":"object", "patternProperties":{"^.{1,10000}$":false},
            "properties":{"small":pattern_values(&small(count))},
        });
        let contract = prepare(&schema, Limits::default()).unwrap();
        assert!(matches!(
            contract.evaluate(&value(r#"{"a":1}"#)),
            ValueOutcome::Fails { .. }
        ));
        let mut patterns = vec!["^.{1,10000}0$".to_owned()];
        patterns.extend(small(count));
        let contract = prepare(&pattern_values(&patterns), Limits::default()).unwrap();
        assert!(satisfies(&contract, r#"{"p0":"a0"}"#));
        if count > 0 {
            assert!(satisfies(&contract, r#"{"p1":"b"}"#));
        }
    }
}

#[test]
fn prepared_programs_have_the_same_outcomes_in_every_call_order() {
    let patterns = ["^.{1,1000}a$".to_owned(), "^.{1,3000}b$".to_owned()];
    for order in [
        [r#"{"p0":"xa"}"#, r#"{"p1":"xb"}"#],
        [r#"{"p1":"xb"}"#, r#"{"p0":"xa"}"#],
    ] {
        let contract = prepare(&pattern_values(&patterns), memory(8 * MIB)).unwrap();
        for text in order {
            assert!(satisfies(&contract, text));
        }
    }
    let schema = json!({
        "type":"object", "patternProperties":{"^.{1,3000}b$":false},
        "properties":{"p0":{"pattern":"^.{1,1000}a$"}},
    });
    let contract = prepare(&schema, memory(8 * MIB)).unwrap();
    assert!(satisfies(&contract, r#"{"p0":"xa"}"#));
    assert!(matches!(
        contract.evaluate(&value(r#"{"xb":1}"#)),
        ValueOutcome::Fails { .. }
    ));
    let patterns = [
        "^.{1,1000}a$".to_owned(),
        "^.{1,3000}b$".to_owned(),
        "^.{1,3000}c$".to_owned(),
    ];
    for first in [r#"{"p0":"xa"}"#, r#"{"p2":"xc"}"#] {
        let contract = prepare(&pattern_values(&patterns), memory(8 * MIB)).unwrap();
        assert!(satisfies(&contract, first));
        for text in [r#"{"p0":"xa"}"#, r#"{"p1":"xb"}"#, r#"{"p2":"xc"}"#] {
            assert!(satisfies(&contract, text));
        }
    }
}

#[test]
fn shared_preparation_exhaustion_cannot_be_deferred_to_first_use() {
    let patterns: Vec<String> = (0..40).map(|i| format!("^.{{1,10000}}{i}$")).collect();
    let one = prepare(&pattern_values(&patterns[..1]), Limits::default()).unwrap();
    assert!(satisfies(&one, r#"{"p0":"a0"}"#));
    let failure = prepare(&pattern_values(&patterns), Limits::default()).unwrap_err();
    assert_eq!(
        failure.kind,
        PreparationFailureKind::Limit {
            resource: "pattern-construction-helper-payload".into(),
        }
    );
}

#[test]
fn the_footprint_counts_unique_programs_without_fixed_slots() {
    let compiled = |count| {
        (0..count)
            .map(|i| json!({"pattern":format!("a{i}|b")}))
            .collect::<Vec<_>>()
    };
    let schema = |items| json!({"prefixItems":items});
    let old_three_slots = memory(3 * 64 * KIB);
    for count in [3, 4] {
        prepare(&schema(compiled(count)), old_three_slots.clone()).unwrap();
    }
    let runs = (0..100)
        .map(|i| json!({"pattern":format!("^a{{{i}}}$")}))
        .collect::<Vec<_>>();
    prepare(
        &schema([compiled(2), runs].concat()),
        old_three_slots.clone(),
    )
    .unwrap();
    prepare(
        &schema([compiled(3), vec![json!({"pattern":"^a{0}$"})]].concat()),
        old_three_slots.clone(),
    )
    .unwrap();
    let repeated = schema(vec![json!({"pattern":"a|b"}); 100]);
    prepare(&repeated, old_three_slots.clone()).unwrap();
    let names = json!({"patternProperties":{"a|b":true,"c|d":true,"e|f":true,"g|h":true}});
    prepare(&names, old_three_slots).unwrap();
    for count in [1024, 1025] {
        prepare(&schema(compiled(count)), Limits::default()).unwrap();
    }
    let one = minimum_memory(&schema(vec![json!({"pattern":"a|b"})]));
    assert_eq!(minimum_memory(&repeated), one);
    assert!(minimum_memory(&schema(compiled(4))) > minimum_memory(&schema(compiled(3))));
    let contract = prepare(&repeated, memory(one)).unwrap();
    assert!(satisfies(&contract, r#"["a","b"]"#));
    assert!(matches!(
        contract.evaluate(&value(r#"["z"]"#)),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn aggregate_pattern_memory_refuses_before_publishing_any_plan() {
    let small = json!({"patternProperties":{"^.{1,100}x$":false}});
    let contract = prepare(&small, memory(256 * KIB)).unwrap();
    assert!(matches!(
        contract.evaluate(&value(r#"{"ax":1}"#)),
        ValueOutcome::Fails { .. }
    ));
    for schema in [
        json!({"patternProperties":{"^.{1,1000}x$":false}}),
        json!({"pattern":"^.{1,1000}x$"}),
    ] {
        assert_memory_refusal(&prepare(&schema, memory(256 * KIB)).unwrap_err());
    }
    let two = json!({"patternProperties":{"^.{1,1000}x$":false,"^.{1,1000}y$":false}});
    let contract = prepare(&two, memory(2 * MIB)).unwrap();
    assert!(matches!(
        contract.evaluate(&value(r#"{"ax":1}"#)),
        ValueOutcome::Fails { .. }
    ));
    for schema in [
        json!({"pattern":"a|b"}),
        json!({"pattern":"^[a-z]+$"}),
        json!({"pattern":"^(?=a)a"}),
    ] {
        assert_memory_refusal(&prepare(&schema, memory(0)).unwrap_err());
    }
    let unmatched = prepare(&json!({"pattern":"^(?=a)a"}), memory(1024)).unwrap();
    assert_eq!(
        no_verdict(&unmatched, r#""a""#).code,
        "pattern-backtracking"
    );
    prepare(&json!({"type":"string"}), memory(0)).unwrap();
}

// A run matcher holds a step for each change of set and a byte for each byte
// of literal text, so a long literal pattern holds about its own length, and
// the pattern memory counts it. 380 distinct literal patterns of 10,002 bytes,
// 3.8 MB within the default total_pattern_bytes, each hold 10,128 bytes:
// 3.7 MiB, within the default memory, where they once held about 1 MB each.
#[test]
fn run_matchers_hold_about_their_text_of_the_pattern_memory() {
    let literal = |i: usize| format!("^t{i:03}x{}$", "a".repeat(9_995));
    let patterns: Vec<String> = (0..380).map(literal).collect();
    assert!(patterns.iter().all(|pattern| pattern.len() == 10_002));
    let contract = prepare(&pattern_values(&patterns), Limits::default()).unwrap();
    let text = &patterns[7][1..10_001];
    assert!(satisfies(&contract, &json!({ "p7": text }).to_string()));
    assert!(matches!(
        contract.evaluate(&value(&json!({ "p7": &text[1..] }).to_string())),
        ValueOutcome::Fails { .. }
    ));
    // No quarter is reserved for caches in the immutable-program footprint.
    for count in [310, 311, 380] {
        prepare(&pattern_values(&patterns[..count]), memory(4 * MIB)).unwrap();
    }
}
