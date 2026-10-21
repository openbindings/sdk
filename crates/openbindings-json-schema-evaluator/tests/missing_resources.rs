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
fn prepare(schema: &str, completion: Option<&str>) -> Result<PreparedValueContract, NoVerdict> {
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant");
    let resources = ResourceSet::new(completion.map(|schema| SchemaResource {
        uri: MISSING.into(),
        document: JsonValue::parse(schema).unwrap(),
    }))
    .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation
        .input()
        .unwrap()
        .prepare_with(&DefaultEvaluator::new(), &resources)
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
fn check(schema: &str) -> Result<(usize, usize), NoVerdict> {
    let stubbed = prepare(schema, None)?;
    let completions: Vec<_> = COMPLETIONS
        .iter()
        .map(|c| prepare(schema, Some(c)).unwrap_or_else(|d| panic!("{schema} with {c}: {d:?}")))
        .collect();
    let (mut decided, mut undecided) = (0, 0);
    for value in VALUES {
        let value = JsonValue::parse(value).unwrap();
        let got = verdict(stubbed.validate(&value));
        let explained = verdict(stubbed.explain(&value));
        let answers: Vec<char> = completions
            .iter()
            .map(|c| verdict(c.validate(&value)))
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
fn refused_for_the_resource(detail: &NoVerdict) -> bool {
    detail.code == "resource-unavailable"
        && detail.location
            == Some(SchemaLocation::new(
                Some(MISSING.into()),
                Pointer::from_utf16(&[]).unwrap(),
            ))
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
    // The seven contracts whose unevaluated keywords' `anyOf`, `oneOf`, `if`,
    // `then`, `allOf` or `dependentSchemas` reach the stub refuse; the other 17
    // decide 106 of their 187 values.
    assert_eq!((decided, undecided, refused), (106, 81, 7));
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
        match check(schema) {
            Err(detail) => assert!(refused_for_the_resource(&detail), "{schema}: {detail:?}"),
            Ok(counts) => panic!("{schema} prepared: {counts:?}"),
        }
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
