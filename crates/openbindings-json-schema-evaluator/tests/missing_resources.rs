//! A contract that reaches a resource that was not supplied prepares with a stub
//! in its place; a value whose evaluation reaches the stub gives no verdict, and
//! any other keeps its verdict. That verdict must be the one every completion of
//! the missing resource gives. A contract in which an `unevaluatedProperties` or
//! `unevaluatedItems` keyword's annotation sources reach the stub refuses
//! preparation instead, since the library reads those sources' structure
//! without evaluating them.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

const MISSING: &str = "https://missing.test/m";
fn prepare(
    schema: &str,
    completion: Option<&str>,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant");
    let resources = ExternalSchemas::new(completion.map(|schema| ExternalSchema {
        uri: MISSING.into(),
        document: JsonValue::parse(schema).unwrap(),
    }))
    .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation.input().unwrap().prepare(
        &DefaultEvaluator::new(),
        &resources,
        &openbindings::PreparationLimits::default(),
    )
}
fn verdict(outcome: ValueOutcome) -> char {
    match outcome {
        ValueOutcome::Satisfies => 'S',
        ValueOutcome::Fails { .. } => 'F',
        ValueOutcome::NoVerdict { detail } => {
            assert_eq!(detail.code, "resource-unavailable");
            'N'
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn unavailable_resources_keep_the_original_reference_location() {
    for external in [
        None,
        Some(r#"{"allOf":[{"$ref":"missing"},{"type":"integer"}]}"#),
    ] {
        let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/a"}}}}"#)
            .unwrap().validate().unwrap().conformant().unwrap();
        let resources = ExternalSchemas::new(external.map(|text| ExternalSchema {
            uri: "https://example.test/a".into(),
            document: JsonValue::parse(text).unwrap(),
        }))
        .unwrap();
        let prepared = document
            .operation("op")
            .unwrap()
            .input()
            .unwrap()
            .prepare(
                &DefaultEvaluator::new(),
                &resources,
                &PreparationLimits::default(),
            )
            .unwrap();
        let value = JsonValue::parse("1").unwrap();
        for outcome in [prepared.evaluate(&value), prepared.explain(&value)] {
            let ValueOutcome::NoVerdict { detail } = outcome else {
                panic!("expected no verdict")
            };
            assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
            let location = detail.location.unwrap();
            if external.is_some() {
                assert_eq!(
                    location.source_uri.as_deref(),
                    Some("https://example.test/a")
                );
                assert_eq!(location.pointer, "/allOf/0/$ref");
                assert_eq!(
                    resources.locate(&location).unwrap().pointer,
                    Some(location.pointer)
                );
            } else {
                assert_eq!(location.source_uri, None);
                assert_eq!(location.pointer, "/operations/op/input/$ref");
                assert_eq!(
                    document.locate(&location).unwrap().pointer,
                    Some(location.pointer)
                );
            }
            assert!(!detail.message.contains('1'));
        }
    }
}
const COMPLETIONS: [&str; 18] = [
    "true",
    "false",
    r#"{"type":"object"}"#,
    r#"{"properties":{"a":true}}"#,
    r#"{"properties":{"a":{"type":"string"}}}"#,
    r#"{"prefixItems":[true,true]}"#,
    r#"{"required":["a"]}"#,
    r#"{"type":"integer"}"#,
    r#"{"unevaluatedProperties":false}"#,
    r#"{"additionalProperties":true}"#,
    r#"{"unevaluatedProperties":true}"#,
    r#"{"patternProperties":{"":true}}"#,
    r#"{"items":true}"#,
    r#"{"contains":true}"#,
    r#"{"type":"array"}"#,
    r#"{"unevaluatedItems":true}"#,
    r#"{"allOf":[{"properties":{"a":true}}]}"#,
    r##"{"$defs":{"d":{"properties":{"a":true}}},"$ref":"#/$defs/d"}"##,
];
const VALUES: [&str; 11] = [
    "null",
    "1",
    r#""s""#,
    "{}",
    r#"{"a":1}"#,
    r#"{"a":"x","b":2}"#,
    r#"{"o":{"a":1}}"#,
    "[]",
    "[1]",
    "[1,2,3]",
    r#"[{"a":1}]"#,
];
// Prepare `schema` without the resource and with each completion; check every
// verdict the stub gives against every completion's. Returns the stub's
// preparation refusal, or the values it decided and left undecided.
fn check(schema: &str) -> Result<(usize, usize), PreparationFailure> {
    let stubbed = prepare(schema, None)?;
    let completions: Vec<_> = COMPLETIONS
        .iter()
        .map(|c| prepare(schema, Some(c)).unwrap_or_else(|d| panic!("{schema} with {c}: {d:?}")))
        .collect();
    let (mut decided, mut undecided) = (0, 0);
    for value in VALUES {
        let value = JsonValue::parse(value).unwrap();
        let got = verdict(stubbed.evaluate(&value));
        let explained = verdict(stubbed.explain(&value));
        let answers: Vec<char> = completions
            .iter()
            .map(|c| verdict(c.evaluate(&value)))
            .collect();
        if got == 'N' {
            undecided += 1;
        } else {
            decided += 1;
            assert!(
                answers.iter().all(|a| *a == got),
                "{schema} {}: {got} vs {answers:?}",
                value.text()
            );
            assert!(
                explained == got || explained == 'N',
                "{schema} {}: explained {explained}",
                value.text()
            );
        }
    }
    Ok((decided, undecided))
}
fn refused_for_the_resource(detail: &PreparationFailure) -> bool {
    detail.kind
        == PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::ResourceUnavailable {
                uri: MISSING.into(),
            },
        }
        && detail.origin.is_none()
}

#[test]
fn a_verdict_given_without_the_resource_is_every_completions_verdict() {
    let m = format!(r#"{{"$ref":"{MISSING}"}}"#);
    let schemas = [
        format!(r#"{{"anyOf":[true,{m}]}}"#),
        format!(r#"{{"anyOf":[false,{m}]}}"#),
        format!(r#"{{"anyOf":[{{"type":"object"}},{m}]}}"#),
        format!(r#"{{"allOf":[false,{m}]}}"#),
        format!(r#"{{"allOf":[{{"type":"object"}},{m}]}}"#),
        format!(r#"{{"oneOf":[true,{m}]}}"#),
        format!(r#"{{"oneOf":[{{"type":"object"}},{m}]}}"#),
        format!(r#"{{"not":{m}}}"#),
        format!(r#"{{"if":{m},"then":false}}"#),
        format!(r#"{{"if":{{"type":"object"}},"then":{m},"else":true}}"#),
        format!(r#"{{"properties":{{"p":{m}}}}}"#),
        format!(r#"{{"items":{m}}}"#),
        format!(r#"{{"contains":{m}}}"#),
        format!(r#"{{"propertyNames":{m}}}"#),
        format!(r#"{{"dependentSchemas":{{"a":{m}}}}}"#),
        format!(r#"{{"anyOf":[true,{m}],"unevaluatedProperties":false}}"#),
        format!(r#"{{"oneOf":[{{"type":"array"}},{m}],"unevaluatedProperties":false}}"#),
        format!(r#"{{"if":{{"type":"object"}},"then":{m},"unevaluatedProperties":false}}"#),
        format!(r#"{{"if":{m},"unevaluatedProperties":false}}"#),
        format!(r#"{{"dependentSchemas":{{"a":{m}}},"unevaluatedProperties":false}}"#),
        format!(r#"{{"anyOf":[true,{m}],"unevaluatedItems":false}}"#),
        format!(
            r##"{{"allOf":[{{"$ref":"#/operations/op/input/$defs/x"}}],"$defs":{{"x":{{"anyOf":[true,{m}]}}}},"unevaluatedProperties":false}}"##
        ),
        format!(r#"{{"properties":{{"a":{{"anyOf":[true,{m}]}}}},"unevaluatedProperties":false}}"#),
        format!(r#"{{"not":{{"anyOf":[true,{m}]}}}}"#),
    ];
    let (mut decided, mut undecided, mut refused) = (0, 0, 0);
    for schema in &schemas {
        match check(schema) {
            Ok((yes, no)) => {
                decided += yes;
                undecided += no;
            }
            Err(detail) => {
                assert!(refused_for_the_resource(&detail), "{schema}: {detail:?}");
                refused += 1;
            }
        }
    }
    // All original templates now prepare; each returned verdict must agree
    // with every independently supplied concrete completion checked above.
    assert_eq!(refused, 0);
    assert_eq!(decided + undecided, schemas.len() * 11);
    assert!(decided >= 106); // Preserve all earlier established decisions.
}

// Each template puts an unevaluated keyword beside the stub, directly or
// through applicators, and is checked against every completion. A template
// whose keyword's annotation sources reach the stub refuses; the others decide
// only what every completion decides alike.
#[test]
fn unevaluated_keywords_beside_a_stub_refuse_or_agree_with_every_completion() {
    let m = format!(r#""$ref":"{MISSING}""#);
    let refusing = [
        format!(r#"{{{m},"unevaluatedProperties":false}}"#),
        format!(r#"{{{m},"unevaluatedItems":false}}"#),
        format!(r#"{{{m},"properties":{{"a":true}},"unevaluatedProperties":false}}"#),
        format!(r#"{{"allOf":[{{{m},"unevaluatedProperties":false}}]}}"#),
        format!(r#"{{"not":{{{m},"unevaluatedProperties":false}}}}"#),
        format!(r#"{{"anyOf":[{{{m},"unevaluatedProperties":false}},{{"type":"string"}}]}}"#),
        format!(r#"{{"anyOf":[{{{m}}},true],"unevaluatedProperties":false}}"#),
        format!(r#"{{"if":true,"then":{{{m}}},"unevaluatedProperties":false}}"#),
        format!(r#"{{"dependentSchemas":{{"a":{{{m}}}}},"unevaluatedProperties":false}}"#),
        format!(r#"{{{m},"unevaluatedProperties":{{"type":"string"}}}}"#),
        format!(r#"{{"properties":{{"o":{{{m},"unevaluatedProperties":false}}}}}}"#),
        format!(r#"{{"oneOf":[{{{m},"unevaluatedProperties":false}},{{"type":"array"}}]}}"#),
        format!(r#"{{"items":{{{m},"unevaluatedProperties":false}}}}"#),
        // A chain of references to the stub.
        format!(
            r##"{{"$ref":"#/operations/op/input/$defs/a","$defs":{{"a":{{"$ref":"#/operations/op/input/$defs/b"}},"b":{{{m}}}}},"unevaluatedProperties":false}}"##
        ),
    ];
    for schema in &refusing {
        let (decided, undecided) = check(schema).unwrap_or_else(|d| panic!("{schema}: {d:?}"));
        assert_eq!(decided + undecided, 11);
    }
    let deciding = [
        format!(r#"{{"contains":{{{m}}},"unevaluatedItems":false}}"#),
        format!(r#"{{"prefixItems":[{{{m}}}],"unevaluatedItems":false}}"#),
        format!(r#"{{"properties":{{"a":{{{m}}}}},"unevaluatedProperties":false}}"#),
    ];
    for schema in &deciding {
        let (decided, _) = check(schema).unwrap_or_else(|d| panic!("{schema}: {d:?}"));
        assert!(decided > 0, "{schema}");
    }
}

// Two established matching branches independently prove oneOf false, regardless
// of an unavailable third resource. Explain can name that original assertion.
#[test]
fn an_independent_one_of_witness_survives_a_missing_resource() {
    let schema =
        format!(r#"{{"oneOf":[{{"type":"integer"}},{{"minimum":0}},{{"$ref":"{MISSING}"}}]}}"#);
    let value = JsonValue::parse("1").unwrap();
    let stubbed = prepare(&schema, None).unwrap();
    assert!(matches!(
        stubbed.evaluate(&value),
        ValueOutcome::Fails { .. }
    ));
    match stubbed.explain(&value) {
        ValueOutcome::Fails {
            problems,
            problems_complete,
            ..
        } => {
            assert!(problems_complete);
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].code, "oneOf");
            assert_eq!(problems[0].schema.pointer, "/operations/op/input/oneOf");
        }
        other => panic!("{other:?}"),
    }
    // With the resource supplied, the same explanation names the `oneOf`.
    let supplied = prepare(&schema, Some("true")).unwrap();
    match supplied.explain(&value) {
        ValueOutcome::Fails { problems, .. } => {
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].code, "oneOf");
        }
        other => panic!("{other:?}"),
    }
}
