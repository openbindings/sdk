//! A stand-in takes the place of what the evaluator cannot evaluate: a
//! resource that was not supplied, a schema that would apply again without
//! consuming the instance, or a pattern it does not match. A value whose
//! verdict depends on how the stand-in comes out gives no verdict naming it,
//! and every other value keeps the verdict that holds however it comes out.
//!
//! Each shape here puts a stand-in in one keyword context, and each of its
//! completions replaces the stand-in with a schema holding at an arbitrary set
//! of the places it can meet: the instance values (a resource or a cycle), the
//! strings (a pattern) or the member names (`patternProperties`). Every
//! verdict given with the stand-in must be every completion's, in either order
//! of the keywords around it.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};

const MISSING: &str = "https://missing.test/m";
const LOOP: &str = "#/operations/op/input/$defs/L";

fn prepare(
    schema: &Value,
    resources: &[(&str, Value)],
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    let resources = ExternalSchemas::new(resources.iter().map(|(uri, schema)| ExternalSchema {
        uri: (*uri).into(),
        document: JsonValue::parse(schema.to_string()).unwrap(),
    }))
    .unwrap();
    document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::new(), &resources)
}
fn verdict(outcome: &ValueOutcome) -> Option<bool> {
    match outcome {
        ValueOutcome::Satisfies => Some(true),
        ValueOutcome::Fails { .. } => Some(false),
        _ => None,
    }
}

// The keyword contexts a stand-in `x` is put in.
fn contexts(x: &Value) -> Vec<Value> {
    let a = json!({"type": "integer"});
    let b = json!({"required": ["a"]});
    vec![
        x.clone(),
        json!({"not": x}),
        json!({"allOf": [x, a]}),
        json!({"allOf": [a, x]}),
        json!({"anyOf": [x, a]}),
        json!({"anyOf": [a, x]}),
        json!({"oneOf": [x, b]}),
        json!({"oneOf": [b, x]}),
        json!({"oneOf": [{"items": x}, {"contains": x}]}),
        json!({"if": x, "then": a, "else": b}),
        json!({"if": b, "then": x}),
        json!({"not": {"anyOf": [x, b]}}),
        json!({"properties": {"a": x}}),
        json!({"additionalProperties": x}),
        json!({"propertyNames": x}),
        json!({"items": x}),
        json!({"contains": x, "maxContains": 1}),
        json!({"contains": x, "minContains": 2}),
        json!({"dependentSchemas": {"a": x}}),
        json!({"items": {"oneOf": [x, a]}}),
        json!({"properties": {"a": x}, "unevaluatedProperties": false}),
        json!({"anyOf": [x, {"type": "object"}], "unevaluatedProperties": false}),
        json!({"anyOf": [{"properties": {"a": x}}, true], "unevaluatedProperties": false}),
        json!({"contains": x, "unevaluatedItems": false}),
    ]
}
const VALUES: [&str; 9] = [
    "null",
    "1",
    r#""a""#,
    "{}",
    r#"{"a":1}"#,
    r#"{"a":"x","b":2}"#,
    "[1]",
    r#"[1,"a"]"#,
    r#"[{"a":2}]"#,
];
// Each value a schema can be applied to within `value`, and each string and
// member name it holds, distinct, in order.
fn places(
    value: &Value,
    values: &mut Vec<Value>,
    strings: &mut Vec<String>,
    names: &mut Vec<String>,
) {
    if !values.contains(value) {
        values.push(value.clone());
    }
    match value {
        Value::String(text) if !strings.contains(text) => strings.push(text.clone()),
        Value::Object(members) => {
            for (name, member) in members {
                for list in [&mut *strings, &mut *names] {
                    if !list.contains(name) {
                        list.push(name.clone());
                    }
                }
                places(member, values, strings, names);
            }
        }
        Value::Array(items) => {
            for item in items {
                places(item, values, strings, names);
            }
        }
        _ => {}
    }
}
fn subsets<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    (0..1u32 << items.len())
        .map(|bits| {
            items
                .iter()
                .enumerate()
                .filter(|(at, _)| bits & (1 << at) != 0)
                .map(|(_, item)| item.clone())
                .collect()
        })
        .collect()
}
// A schema holding exactly at `values`, and a pattern matching exactly
// `strings`.
fn holding(values: &[Value]) -> Value {
    if values.is_empty() {
        json!(false)
    } else {
        json!({ "enum": values })
    }
}
fn matching(strings: &[String]) -> String {
    if strings.is_empty() {
        return "[^\\s\\S]".into();
    }
    let alternatives: Vec<String> = strings
        .iter()
        .map(|text| {
            text.chars()
                .map(|c| format!("\\u{{{:x}}}", c as u32))
                .collect()
        })
        .collect();
    format!("^(?:{})$", alternatives.join("|"))
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Missing,
    Cycle,
    Pattern,
    Names,
}
// The stand-in, and the schema holding the contexts it is put in.
fn stand_in(kind: Kind) -> Value {
    match kind {
        Kind::Missing => json!({"$ref": MISSING}),
        Kind::Cycle => json!({"$ref": LOOP}),
        Kind::Pattern => json!({"pattern": "("}),
        Kind::Names => {
            json!({"patternProperties": {"(": {"type": "string"}}, "additionalProperties": {"type": "integer"}})
        }
    }
}
fn holder(kind: Kind, context: Value, completion: Option<&Value>) -> Value {
    match kind {
        // The cycle's schema applies again at the instance it holds.
        Kind::Cycle => {
            let body = completion.cloned().unwrap_or_else(|| json!({"$ref": LOOP}));
            json!({"$defs": {"L": {"anyOf": [{"minimum": 2}, body]}}, "allOf": [context]})
        }
        _ => context,
    }
}

#[test]
fn verdicts_given_with_a_stand_in_are_every_completions() {
    let (mut decided, mut undecided, mut refused) = (0, 0, 0);
    for kind in [Kind::Missing, Kind::Cycle, Kind::Pattern, Kind::Names] {
        for (at, context) in contexts(&stand_in(kind)).into_iter().enumerate() {
            let contract = match prepare(&holder(kind, context.clone(), None), &[]) {
                Ok(contract) => contract,
                Err(detail) => {
                    // Only an unevaluated keyword reading the stand-in refuses.
                    assert!(
                        context.to_string().contains("unevaluated"),
                        "{kind:?} {at}: {detail:?}"
                    );
                    refused += 1;
                    continue;
                }
            };
            for text in VALUES {
                let value = JsonValue::parse(text).unwrap();
                let outcome = contract.evaluate(&value);
                let explained = contract.explain(&value);
                assert_eq!(
                    verdict(&outcome),
                    verdict(&explained),
                    "{kind:?} {at} {text}"
                );
                let Some(got) = verdict(&outcome) else {
                    match (kind, &outcome) {
                        (Kind::Missing, ValueOutcome::NoVerdict { detail }) => {
                            assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable)
                        }
                        (Kind::Cycle, ValueOutcome::NoVerdict { detail }) => {
                            assert_eq!(detail.reason, NoVerdictReason::UnsupportedCapability)
                        }
                        (Kind::Pattern | Kind::Names, ValueOutcome::Undefined { .. }) => {}
                        _ => panic!("{kind:?} {at} {text}: {outcome:?}"),
                    }
                    undecided += 1;
                    continue;
                };
                decided += 1;
                let parsed: Value = serde_json::from_str(text).unwrap();
                let (mut values, mut strings, mut names) = (Vec::new(), Vec::new(), Vec::new());
                places(&parsed, &mut values, &mut strings, &mut names);
                let completions: Vec<(Value, Vec<(&str, Value)>)> = match kind {
                    Kind::Missing => subsets(&values)
                        .iter()
                        .map(|set| {
                            (
                                holder(kind, context.clone(), None),
                                vec![(MISSING, holding(set))],
                            )
                        })
                        .collect(),
                    Kind::Cycle => subsets(&values)
                        .iter()
                        .map(|set| (holder(kind, context.clone(), Some(&holding(set))), vec![]))
                        .collect(),
                    Kind::Pattern => subsets(&strings)
                        .iter()
                        .map(|set| {
                            (
                                contexts(&json!({"pattern": matching(set)}))[at].clone(),
                                vec![],
                            )
                        })
                        .collect(),
                    Kind::Names => subsets(&names)
                        .iter()
                        .map(|set| {
                            let mut names = stand_in(kind);
                            names["patternProperties"] =
                                json!({ matching(set): {"type": "string"} });
                            (contexts(&names)[at].clone(), vec![])
                        })
                        .collect(),
                };
                for (schema, resources) in completions {
                    let completed = prepare(&schema, &resources).unwrap();
                    assert_eq!(
                        verdict(&completed.evaluate(&value)),
                        Some(got),
                        "{kind:?} {at} {text}: {schema}"
                    );
                }
            }
        }
    }
    assert_eq!(refused, 0);
    assert_eq!(
        decided + undecided,
        4 * contexts(&json!(true)).len() * VALUES.len()
    );
    assert!(decided >= 678); // Every formerly established verdict remains checked.
}

// A decided failure is explained at the keyword that fails however the
// stand-in comes out, at its original location.
#[test]
fn a_decided_failure_names_the_keyword_that_decides_it() {
    for (schema, keyword) in [
        (
            json!({"allOf": [{"$ref": MISSING}, {"type": "integer"}]}),
            "/allOf/1/type",
        ),
        (
            json!({"allOf": [{"type": "integer"}, {"pattern": "("}]}),
            "/allOf/0/type",
        ),
        (
            json!({"oneOf": [{"type": "integer"}, {"$ref": MISSING}, {"minimum": 0}]}),
            "/oneOf",
        ),
        (
            json!({"if": {"$ref": MISSING}, "then": {"type": "integer"}, "else": {"type": "integer"}}),
            "/if",
        ),
    ] {
        let contract = prepare(&schema, &[]).unwrap();
        let value = JsonValue::parse(if keyword == "/oneOf" { "1" } else { r#""a""# }).unwrap();
        let ValueOutcome::Fails { problems, .. } = contract.explain(&value) else {
            panic!("{schema}")
        };
        if keyword == "/if" {
            // Both branches fail, but if itself asserts nothing. The bounded
            // witness pass may omit this correlation-only proof.
            assert!(problems.is_empty(), "{schema}");
            continue;
        }
        assert_eq!(problems.len(), 1, "{schema}");
        assert_eq!(
            problems[0].schema,
            SchemaOrigin::new(
                None,
                format!("/operations/op/input{keyword}").parse().unwrap()
            ),
            "{schema}"
        );
    }
}

// The stand-in for a cycle is where a schema would apply again to the
// instance it is already applied to: a value decided on the way there keeps
// its verdict, whichever schema of the cycle evaluation enters first.
#[test]
fn a_cycle_stands_in_where_a_schema_would_apply_again() {
    let a = "#/operations/op/input/$defs/A";
    let b = "#/operations/op/input/$defs/B";
    let defs = json!({
        "A": {"anyOf": [{"type": "string"}, {"$ref": b}]},
        "B": {"type": "object", "$ref": a},
    });
    for (entry, location) in [
        (a, "/operations/op/input/$defs/A"),
        (b, "/operations/op/input/$defs/B"),
    ] {
        let contract = prepare(&json!({"$defs": defs, "$ref": entry}), &[]).unwrap();
        // 5 is no string and no object, so it fails before A applies again.
        assert!(matches!(
            contract.evaluate(&JsonValue::parse("5").unwrap()),
            ValueOutcome::Fails { .. }
        ));
        let ValueOutcome::NoVerdict { detail } =
            contract.evaluate(&JsonValue::parse("{}").unwrap())
        else {
            panic!("{entry}")
        };
        assert_eq!(detail.code, "semantic-analysis-inconclusive");
        assert_eq!(detail.reason, NoVerdictReason::UnsupportedCapability);
        assert_eq!(
            detail.location,
            Some(SchemaOrigin::new(None, location.parse().unwrap()))
        );
    }
    let contract = prepare(&json!({"$defs": defs, "$ref": a}), &[]).unwrap();
    assert!(matches!(
        contract.evaluate(&JsonValue::parse(r#""s""#).unwrap()),
        ValueOutcome::Satisfies
    ));
}

// Eight schemas that each name all the others in an `anyOf` have more paths
// than the unfolding copies: each of their same-instance references stands in
// instead, which still decides what holds however the cycle comes out.
#[test]
fn a_dense_cycle_stands_in_at_each_reference() {
    let names: Vec<String> = (0..8).map(|i| format!("s{i}")).collect();
    let mut defs = serde_json::Map::new();
    for name in &names {
        let mut branches = vec![json!({"type": "integer"})];
        branches.extend(
            names
                .iter()
                .filter(|other| *other != name)
                .map(|other| json!({"$ref": format!("#/operations/op/input/$defs/{other}")})),
        );
        defs.insert(name.clone(), json!({"anyOf": branches}));
    }
    let contract = prepare(
        &json!({"$defs": defs, "$ref": "#/operations/op/input/$defs/s0"}),
        &[],
    )
    .unwrap();
    assert!(matches!(
        contract.evaluate(&JsonValue::parse("1").unwrap()),
        ValueOutcome::Satisfies
    ));
    let ValueOutcome::NoVerdict { detail } =
        contract.evaluate(&JsonValue::parse(r#""x""#).unwrap())
    else {
        panic!()
    };
    assert_eq!(detail.code, "semantic-analysis-inconclusive");
}

// Unevaluated keywords consume coherent annotations from successful branches.
// Known undefined matches and uncomputed predicates have distinct outcomes.
#[test]
fn unevaluated_annotation_dependence_is_evaluated_per_value() {
    for (source, required) in [
        (json!({"$ref":"#/operations/op/input"}), "unsupported"),
        (json!({"patternProperties":{"(":true}}), "undefined"),
        (json!({"patternProperties":{"(?=a)a":true}}), "unsupported"),
        (json!({"$ref":MISSING}), "external"),
        (json!({"properties":{"a":{"pattern":"("}}}), "satisfies"),
    ] {
        let schema = json!({"anyOf":[source,{"type":"object"}],"unevaluatedProperties":false});
        let contract = prepare(&schema, &[]).unwrap();
        let value = JsonValue::parse(r#"{"a":1}"#).unwrap();
        for outcome in [contract.evaluate(&value), contract.explain(&value)] {
            let actual = match outcome {
                ValueOutcome::Satisfies => "satisfies",
                ValueOutcome::Undefined { .. } => "undefined",
                ValueOutcome::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::ResourceUnavailable =>
                {
                    "external"
                }
                ValueOutcome::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::UnsupportedCapability =>
                {
                    "unsupported"
                }
                other => panic!("{schema}: {other:?}"),
            };
            assert_eq!(actual, required, "{schema}");
        }
    }
}

// A member that `additionalProperties` reaches might match a name the
// evaluator does not match, so its verdict holds only where it holds both
// ways: `additionalProperties: false` fails a member the name's subschema
// fails, and gives no verdict for one it accepts.
#[test]
fn additional_properties_beside_an_unmatched_name() {
    let contract = prepare(
        &json!({"patternProperties": {"(": {"type": "integer"}}, "additionalProperties": false}),
        &[],
    )
    .unwrap();
    for (value, expected) in [
        (r#"{"a":"x"}"#, Some(false)),
        (r#"{"a":1}"#, None),
        ("{}", Some(true)),
    ] {
        let value = JsonValue::parse(value).unwrap();
        assert_eq!(verdict(&contract.evaluate(&value)), expected);
        assert_eq!(verdict(&contract.explain(&value)), expected);
    }
}

// A copy written from both bounds of its subschemas refers to both copies of
// each, so nested `oneOf`, `if` and counted `contains` keywords would double a
// bound's work at every level. A bound's pass does at most a multiple of the
// verdict pass's work, so thirty nested levels over a stand-in give its
// no-verdict as quickly as the verdict pass does, where doubling would take
// days.
#[test]
fn nested_keywords_written_from_both_bounds_cost_a_multiple_of_the_verdict_pass() {
    let leaves = [
        (json!({"$ref": MISSING}), "resource-unavailable"),
        (json!({"pattern": "(?=a)a"}), "pattern-backtracking"),
        (
            json!({"$ref": "#/operations/op/input"}),
            "semantic-analysis-inconclusive",
        ),
    ];
    let wraps: [fn(Value) -> Value; 3] = [
        |inner| json!({"oneOf": [inner, {"type": "null"}]}),
        |inner| json!({"if": inner, "then": {"type": "string"}, "else": {"type": "integer"}}),
        |inner| json!({"anyOf": [{"type": "string"}, {"contains": inner, "maxContains": 1}]}),
    ];
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for (leaf, code) in leaves {
            for wrap in wraps {
                let mut schema = leaf.clone();
                for _ in 0..30 {
                    schema = wrap(schema);
                }
                let contract = prepare(&schema, &[]).unwrap();
                for value in [r#""a""#, r#"["a"]"#] {
                    let value = JsonValue::parse(value).unwrap();
                    for outcome in [contract.evaluate(&value), contract.explain(&value)] {
                        if let ValueOutcome::NoVerdict { detail } = outcome {
                            assert_eq!(detail.code, code, "{schema}");
                        }
                    }
                }
            }
        }
        sent.send(()).unwrap();
    });
    received
        .recv_timeout(std::time::Duration::from_secs(120))
        .expect("bounded bound passes");
}

// Two known matching branches prove oneOf false independently of the third
// branch's invalid pattern, recursion, unavailable resource or unsupported match.

#[test]
fn an_independent_one_of_witness_does_not_depend_on_the_third_branch() {
    let value = JsonValue::parse(r#""a""#).unwrap();
    for third in [
        json!({"pattern": "["}),
        json!({"$ref": "#/operations/op/input"}),
        json!({"$ref": MISSING}),
        json!({"pattern": "(?=a)a"}),
    ] {
        let schema = json!({"oneOf": [{"type": "string"}, {"type": "string"}, third]});
        let contract = prepare(&schema, &[]).unwrap();
        assert!(matches!(
            contract.evaluate(&value),
            ValueOutcome::Fails { .. }
        ));
        let ValueOutcome::Fails { problems, .. } = contract.explain(&value) else {
            panic!("{schema}")
        };
        assert_eq!(problems.len(), 1, "{schema}");
        assert_eq!(
            problems[0].schema,
            SchemaOrigin::new(None, "/operations/op/input/oneOf".parse().unwrap()),
            "{schema}"
        );
    }
}
