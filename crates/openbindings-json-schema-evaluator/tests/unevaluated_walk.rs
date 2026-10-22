//! The unevaluated-walk transform (`src/unevaluated_walk.rs`) is a stopgap for an
//! upstream bug in jsonschema 0.58.6: the library's `unevaluatedProperties` walk
//! compiles each `patternProperties` subschema it reaches and drops the compiled
//! node, so a `$ref` target a later compilation round points at can be gone when
//! validation reaches it, and validation panics. The transform depends on which
//! keywords that walk follows. These tests fail loudly when a library upgrade
//! changes either fact: the bug reproduces on the program the evaluator bundles,
//! and the transformed program, whose subschemas sit under `not`, does not panic.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
struct Resource {
    uri: String,
    schema: Box<RawValue>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    // Raw text keeps each schema's keyword order, which decides what the library compiles first.
    document: Box<RawValue>,
    resources: Vec<Resource>,
    values: Vec<Box<RawValue>>,
    expected: Vec<String>,
}

fn outcome(outcome: &ValueOutcome) -> &'static str {
    match outcome {
        ValueOutcome::Satisfies => "S",
        ValueOutcome::Fails { .. } => "F",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "N",
    }
}

#[test]
fn pattern_subschemas_reached_through_references_stay_evaluable() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/unevaluated-walk-placements.json")).unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let parsed = Document::parse(case.document.get()).unwrap();
        let validation = parsed.validate().unwrap();
        let proof = validation.conformant().expect("conformant fixture");
        let resources = ResourceSet::new(case.resources.iter().map(|r| SchemaResource {
            uri: r.uri.clone(),
            document: JsonValue::parse(r.schema.get()).unwrap(),
        }))
        .unwrap();
        let contract = proof
            .resolve_operation("op")
            .expect("declared operation")
            .input()
            .expect("declared input")
            .prepare_with(&DefaultEvaluator::new(), &resources)
            .unwrap_or_else(|detail| panic!("{}: {detail:?}", case.id));
        for (value, expected) in case.values.iter().zip(&case.expected) {
            let value = JsonValue::parse(value.get()).unwrap();
            let label = format!("{} {}", case.id, value.text());
            assert_eq!(outcome(&contract.validate(&value)), expected, "{label}");
            assert_eq!(outcome(&contract.explain(&value)), expected, "{label}");
        }
    }
}

#[derive(Deserialize)]
struct Programs {
    untransformed: serde_json::Value,
    transformed: serde_json::Value,
    values: Vec<serde_json::Value>,
    valid: Vec<bool>,
}
fn programs() -> Programs {
    serde_json::from_str(include_str!("fixtures/unevaluated-walk-programs.json")).unwrap()
}
// The library's own verdicts on a bundled program, `None` where it panics.
fn library_verdicts(
    program: &serde_json::Value,
    values: &[serde_json::Value],
) -> Vec<Option<bool>> {
    let validator = jsonschema::options().build(program).unwrap();
    values
        .iter()
        .map(|value| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| validator.is_valid(value)))
                .ok()
        })
        .collect()
}
#[test]
fn the_library_still_drops_pattern_subschemas_its_walk_compiles() {
    let programs = programs();
    let verdicts = library_verdicts(&programs.untransformed, &programs.values);
    assert!(
        verdicts.iter().any(Option::is_none),
        "jsonschema no longer panics on the bundled reproducer ({verdicts:?}); \
         the unevaluated-walk transform may no longer be needed"
    );
}
#[test]
fn the_walk_does_not_enter_not_so_the_transformed_program_evaluates() {
    let programs = programs();
    let verdicts = library_verdicts(&programs.transformed, &programs.values);
    let expected: Vec<_> = programs.valid.iter().copied().map(Some).collect();
    assert_eq!(
        verdicts, expected,
        "the transformed reproducer panics or decides differently; the library's \
         unevaluatedProperties walk may now enter `not`, which the transform relies on"
    );
}

// A failure in a subschema the transform moved is explained at the original
// `patternProperties` keyword, for the object holding the member.
#[test]
fn moved_subschema_failures_are_explained_at_their_pattern_properties() {
    let document = Document::parse(
        r##"{"openbindings":"0.2.0","schemas":{"p":{"patternProperties":{"^a":{"type":"integer"}}}},"operations":{"op":{"input":{"properties":{"o":{"unevaluatedProperties":false,"$ref":"#/schemas/p"}}}}}}"##,
    )
    .unwrap();
    let validation = document.validate().unwrap();
    let proof = validation.conformant().expect("conformant fixture");
    let contract = proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::new())
        .unwrap();
    let value = JsonValue::parse(r#"{"o":{"a":"x"}}"#).unwrap();
    let ValueOutcome::Fails { problems, .. } = contract.explain(&value) else {
        panic!("the member fails its pattern's schema")
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, "patternProperties");
    assert_eq!(problems[0].instance, "/o");
    assert_eq!(
        problems[0].schema,
        SchemaLocation::new(None, "/schemas/p/patternProperties".parse().unwrap())
    );
    assert!(matches!(
        contract.validate(&JsonValue::parse(r#"{"o":{"a":1}}"#).unwrap()),
        ValueOutcome::Satisfies
    ));
}

// A walk longer than the evaluator admits refuses preparation at the
// unevaluated keyword whose walk it is.
#[test]
fn a_walk_too_deep_refuses_at_its_keyword() {
    let mut defs = serde_json::Map::new();
    for i in 0..20 {
        defs.insert(
            format!("a{i}"),
            serde_json::json!({"allOf": [{"$ref": format!("#/operations/op/input/$defs/a{}", i + 1)}]}),
        );
    }
    defs.insert("a20".into(), serde_json::json!({"type": "object"}));
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"inner": {
            "allOf": [{"$ref": "#/operations/op/input/$defs/a0"}],
            "unevaluatedProperties": false
        }},
        "$defs": defs
    });
    let document = Document::parse(
        serde_json::json!({"openbindings": "0.2.0", "operations": {"op": {"input": schema}}})
            .to_string(),
    )
    .unwrap();
    let proof = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant")
        .clone();
    let operation = proof.resolve_operation("op").unwrap();
    let Err(detail) = operation.input().unwrap().prepare(&DefaultEvaluator::new()) else {
        panic!("a 41-reference walk prepared")
    };
    assert_eq!(detail.code, "unevaluated-walk-limit");
    assert_eq!(
        detail.location,
        Some(SchemaLocation::new(
            None,
            "/operations/op/input/properties/inner/unevaluatedProperties"
                .parse()
                .unwrap(),
        ))
    );
    // The refusal states the arithmetic and the setting to raise.
    assert_eq!(
        detail.message,
        "this keyword's annotation walk weighs 273, more than the 105 its 256 KiB stack_bytes \
         admits: a fifth of the stack is left to the caller, 99 KiB to the rest of preparation, \
         and each unit takes 1 KiB; raise stack_bytes (stackBytes in TypeScript)"
    );
    // A host that gives preparation a larger stack admits the walk.
    let larger = DefaultEvaluator::with_limits(Limits {
        stack_bytes: 1024 * 1024,
        ..Limits::default()
    });
    let input = operation.input().unwrap();
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn_scoped(scope, || assert!(input.prepare(&larger).is_ok()))
            .unwrap()
            .join()
            .unwrap()
    });
}
