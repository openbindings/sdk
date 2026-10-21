//! `Limits::schema_resources` caps the schema resources one contract may reach, counted
//! as its projection holds them: the document resource holding the contract's
//! schema, and each embedded `$id`, supplied and packaged meta-schema resource it
//! reaches. The default sets no cap. A capped contract that reaches more is
//! refused before the library registers any resource, so the refusal stays cheap
//! however many it reaches.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::time::{Duration, Instant};

fn document(schemas: &[String], input: &str) -> String {
    format!(
        r#"{{"openbindings":"0.2.0","schemas":{{{}}},"operations":{{"op":{{"input":{input}}}}}}}"#,
        schemas.join(",")
    )
}
fn prepare_limited(
    text: &str,
    resources: &ResourceSet,
    limits: Limits,
) -> Result<PreparedValueContract, NoVerdict> {
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    proof
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare_with(&DefaultEvaluator::with_limits(limits), resources)
}
fn prepare(
    text: &str,
    resources: &ResourceSet,
    cap: Option<usize>,
) -> Result<PreparedValueContract, NoVerdict> {
    let limits = Limits {
        schema_resources: cap,
        ..Limits::default()
    };
    prepare_limited(text, resources, limits)
}
// An input that references each of `count` supplied integer resources.
fn supplied(count: usize) -> (String, ResourceSet) {
    let uri = |i: usize| format!("https://set.test/r{i}");
    let references: Vec<_> = (0..count)
        .map(|i| format!(r#"{{"$ref":"{}"}}"#, uri(i)))
        .collect();
    let input = format!(r#"{{"allOf":[{}]}}"#, references.join(","));
    let resources = ResourceSet::new((0..count).map(|i| SchemaResource {
        uri: uri(i),
        document: JsonValue::parse(r#"{"type":"integer"}"#).unwrap(),
    }))
    .unwrap();
    (document(&[], &input), resources)
}
// d0 -> d1 -> ... -> d(links), one contained `$id` resource per schema.
fn id_chain(links: usize) -> String {
    let uri = |i: usize| format!("https://chain.test/d{i}");
    let mut schemas: Vec<_> = (0..links)
        .map(|i| format!(r#""d{i}":{{"$id":"{}","$ref":"{}"}}"#, uri(i), uri(i + 1)))
        .collect();
    schemas.push(format!(
        r#""d{links}":{{"$id":"{}","type":"integer"}}"#,
        uri(links)
    ));
    document(&schemas, r##"{"$ref":"#/schemas/d0"}"##)
}
fn assert_refused(refused: NoVerdict) {
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
    assert_eq!(refused.code, "schema-resource-limit");
    assert!(refused.location.is_none());
}
// A cap of `reached` admits the contract, and so does no cap; one less refuses it.
// The admitted contract decides `value`, which satisfies it.
fn assert_reaches_deciding(text: &str, resources: &ResourceSet, reached: usize, value: &str) {
    let admitted = prepare(text, resources, Some(reached)).expect("admitted at its count");
    assert!(matches!(
        admitted.validate(&JsonValue::parse(value).unwrap()),
        ValueOutcome::Satisfies
    ));
    prepare(text, resources, None).expect("admitted without a cap");
    assert_refused(prepare(text, resources, Some(reached - 1)).unwrap_err());
}
fn assert_reaches(text: &str, resources: &ResourceSet, reached: usize) {
    assert_reaches_deciding(text, resources, reached, "7");
}

#[test]
fn the_default_sets_no_resource_cap() {
    assert_eq!(Limits::default().schema_resources, None);
}

#[test]
fn a_cap_admits_exactly_the_resources_a_contract_reaches() {
    let none = ResourceSet::default();
    // The document resource alone.
    assert_reaches(&document(&[], r#"{"type":"integer"}"#), &none, 1);
    // The document resource and each supplied resource.
    let (text, resources) = supplied(40);
    assert_reaches(&text, &resources, 41);
    // The document resource and each contained `$id` resource.
    assert_reaches(&id_chain(30), &none, 32);
    // Supplied resources the contract does not reach are not counted.
    let (_, unreached) = supplied(40);
    assert_reaches(&document(&[], r#"{"type":"integer"}"#), &unreached, 1);
}

#[test]
fn packaged_meta_schema_resources_count_when_reached() {
    // The document resource, the 2020-12 meta-schema and the seven vocabulary
    // meta-schemas it references.
    let text = document(
        &[],
        r#"{"$ref":"https://json-schema.org/draft/2020-12/schema"}"#,
    );
    assert_reaches_deciding(&text, &ResourceSet::default(), 9, r#"{"type":"integer"}"#);
}

#[test]
fn zero_refuses_every_contract() {
    assert_refused(
        prepare(
            &document(&[], r#"{"type":"integer"}"#),
            &ResourceSet::default(),
            Some(0),
        )
        .unwrap_err(),
    );
}

#[test]
fn a_refusal_precedes_preparing_any_resource() {
    // Each resource's patterns are admitted as it is registered, so a pattern
    // over the byte limit refuses a contract within the cap. Over the cap, the
    // same contract refuses for its resources before any of them is prepared.
    let text = document(&[], r#"{"type":"string","pattern":"^(ab)+$"}"#);
    let none = ResourceSet::default();
    let limits = |cap| Limits {
        pattern_bytes: 4,
        schema_resources: cap,
        ..Limits::default()
    };
    let within = prepare_limited(&text, &none, limits(Some(1))).unwrap_err();
    assert_eq!(within.code, "pattern-byte-limit");
    assert_refused(prepare_limited(&text, &none, limits(Some(0))).unwrap_err());
}

// The fastest of three refusals of a contract reaching `count` supplied resources.
fn refusal_time(count: usize) -> Duration {
    let (text, resources) = supplied(count);
    (0..3)
        .map(|_| {
            let start = Instant::now();
            assert_refused(prepare(&text, &resources, Some(count)).unwrap_err());
            start.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
fn a_refusal_takes_time_linear_in_the_resources() {
    // Registering resources takes time quadratic in their number: refused after
    // registration, four times the resources took about thirteen times as long in
    // a debug build. Refused before it, the time grows with the projection, about
    // fourfold.
    let small = refusal_time(2_000);
    let large = refusal_time(8_000);
    let ratio = large.as_secs_f64() / small.as_secs_f64();
    assert!(ratio < 8.0, "{small:?} -> {large:?}: {ratio:.1}x");
}

// The forms the evaluator keeps for a contract's patterns are bounded together:
// patterns each within the per-pattern limit refuse once their forms' total
// passes the contract's, naming the keyword that passed it. These patterns'
// forms are as long as they are.
#[test]
fn a_contract_s_patterns_are_bounded_together() {
    let text = document(
        &[],
        r#"{"properties":{"a":{"pattern":"^aaaaaa$"},"b":{"patternProperties":{"^bbbbbb$":true}},"c":{"pattern":"^cccccc$"}}}"#,
    );
    let none = ResourceSet::default();
    let limits = |total| Limits {
        total_pattern_bytes: total,
        ..Limits::default()
    };
    assert!(prepare_limited(&text, &none, limits(24)).is_ok());
    let refused = prepare_limited(&text, &none, limits(23)).unwrap_err();
    assert_eq!(refused.code, "pattern-total-byte-limit");
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
    let pointer = refused.location.expect("located").pointer.to_string();
    assert!(
        pointer.ends_with("/pattern") || pointer.ends_with("/patternProperties"),
        "{pointer}"
    );
}

// The total counts forms, not patterns: a set's form spells it out, so a
// two-byte `\S` takes far more of the total than its source. A `pattern` kept
// once for every schema holding it counts once; the library matches each
// `patternProperties` name from its own copy of the form, so each counts.
#[test]
fn the_pattern_total_counts_the_forms_kept() {
    let none = ResourceSet::default();
    let limits = |total| Limits {
        total_pattern_bytes: total,
        ..Limits::default()
    };
    let spelled = document(&[], r#"{"pattern":"\\S"}"#);
    assert_eq!(
        prepare_limited(&spelled, &none, limits(64))
            .unwrap_err()
            .code,
        "pattern-total-byte-limit"
    );
    assert!(prepare_limited(&spelled, &none, limits(1024)).is_ok());
    let repeated = document(
        &[],
        r#"{"properties":{"a":{"pattern":"^aaaaaa$"},"b":{"pattern":"^aaaaaa$"},"c":{"pattern":"^aaaaaa$"}}}"#,
    );
    assert!(prepare_limited(&repeated, &none, limits(8)).is_ok());
    let names = document(
        &[],
        r#"{"properties":{"a":{"patternProperties":{"^aaaaaa$":true}},"b":{"patternProperties":{"^aaaaaa$":true}}}}"#,
    );
    assert!(prepare_limited(&names, &none, limits(16)).is_ok());
    assert_eq!(
        prepare_limited(&names, &none, limits(15)).unwrap_err().code,
        "pattern-total-byte-limit"
    );
}
