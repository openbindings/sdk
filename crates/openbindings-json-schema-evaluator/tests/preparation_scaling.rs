//! Preparation bundles the schemas a contract reaches into one resource, so the
//! library compiles each once, whatever reference paths through resources reach
//! it. A dynamic reference is resolved before compilation: the schemas it can
//! reach are specialized once per dynamic context, and a contract whose
//! specialized copies and contexts would grow its program by more than
//! `Limits::program_bytes` refuses.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn prepare_with(text: &str, resources: &ResourceSet) -> Result<PreparedValueContract, NoVerdict> {
    prepare_within(text, resources, Limits::default())
}
fn prepare_within(
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
fn prepare(text: &str) -> Result<PreparedValueContract, NoVerdict> {
    prepare_with(text, &ResourceSet::default())
}
fn document(schemas: &[String], input: &str) -> String {
    format!(
        r#"{{"openbindings":"0.2.0","schemas":{{{}}},"operations":{{"op":{{"input":{input}}}}}}}"#,
        schemas.join(",")
    )
}
fn resources(entries: Vec<(String, String)>) -> ResourceSet {
    ResourceSet::new(entries.into_iter().map(|(uri, text)| SchemaResource {
        uri,
        document: JsonValue::parse(text).unwrap(),
    }))
    .unwrap()
}
// d0 -> d1 -> ... -> d(links), each a same-document reference; the last is an integer.
fn chain(links: usize) -> String {
    let mut schemas: Vec<_> = (0..links)
        .map(|i| format!(r##""d{i}":{{"$ref":"#/schemas/d{}"}}"##, i + 1))
        .collect();
    schemas.push(format!(r#""d{links}":{{"type":"integer"}}"#));
    document(&schemas, r##"{"$ref":"#/schemas/d0"}"##)
}
// The same chain through contained resources, one `$id` per link.
fn resource_chain(links: usize) -> String {
    let mut schemas: Vec<_> = (0..links)
        .map(|i| {
            format!(
                r#""d{i}":{{"$id":"https://chain.test/d{i}","$ref":"https://chain.test/d{}"}}"#,
                i + 1
            )
        })
        .collect();
    schemas.push(format!(
        r#""d{links}":{{"$id":"https://chain.test/d{links}","type":"integer"}}"#
    ));
    document(&schemas, r##"{"$ref":"#/schemas/d0"}"##)
}
// Each level is a contained resource whose two branches, each its own resource,
// both reference the next level: every level doubles the resource paths to it.
// `last` is the final level's body.
fn diamonds_ending(levels: usize, last: &str) -> String {
    let mut schemas = Vec::new();
    for i in 0..levels {
        schemas.push(format!(
            r#""r{i}":{{"$id":"https://diamond.test/r{i}","allOf":[{{"$ref":"https://diamond.test/a{i}"}},{{"$ref":"https://diamond.test/b{i}"}}]}}"#
        ));
        for branch in ["a", "b"] {
            schemas.push(format!(
                r#""{branch}{i}":{{"$id":"https://diamond.test/{branch}{i}","$ref":"https://diamond.test/r{}"}}"#,
                i + 1
            ));
        }
    }
    schemas.push(format!(
        r#""r{levels}":{{"$id":"https://diamond.test/r{levels}",{last}}}"#
    ));
    document(&schemas, r##"{"$ref":"#/schemas/r0"}"##)
}
fn diamonds(levels: usize) -> String {
    diamonds_ending(levels, r#""type":"integer""#)
}
// The same ladder, its last level an integer reached through a dynamic anchor.
fn dynamic_diamonds(levels: usize) -> String {
    diamonds_ending(
        levels,
        r##""$dynamicRef":"#leaf","$defs":{"leaf":{"$dynamicAnchor":"leaf","type":"integer"}}"##,
    )
}
// `scopes` resources that each reference one shared resource requiring `names`
// property names. With `dynamic`, the root declares a dynamic anchor that one of
// its own properties resolves, so the build compiles the shared resource once per
// resource path; otherwise once.
fn required_fan(scopes: usize, names: usize, dynamic: bool) -> String {
    let mut properties: Vec<_> = (0..scopes)
        .map(|i| format!(r#""p{i}":{{"$ref":"https://fan.test/m{i}"}}"#))
        .collect();
    let anchor = if dynamic {
        properties.push(r##""self":{"$dynamicRef":"#node"}"##.into());
        r#""$dynamicAnchor":"node","#
    } else {
        ""
    };
    let mut schemas = vec![format!(
        r#""top":{{"$id":"https://fan.test/top",{anchor}"type":"object","properties":{{{}}}}}"#,
        properties.join(",")
    )];
    for i in 0..scopes {
        schemas.push(format!(
            r#""m{i}":{{"$id":"https://fan.test/m{i}","$ref":"https://fan.test/leaf"}}"#
        ));
    }
    let required: Vec<_> = (0..names).map(|n| format!(r#""n{n}""#)).collect();
    schemas.push(format!(
        r#""leaf":{{"$id":"https://fan.test/leaf","required":[{}]}}"#,
        required.join(",")
    ));
    document(&schemas, r##"{"$ref":"#/schemas/top"}"##)
}
// A domain model in `layers` layers of `width` object schemas, each with six
// string properties and, above the last layer, `fan_out` properties referencing
// schemas of the next layer. `uri(layer, schema)` names each schema.
fn layered(
    layers: usize,
    fan_out: usize,
    width: usize,
    uri: &dyn Fn(usize, usize) -> String,
    with_id: bool,
) -> Vec<(String, String)> {
    let mut schemas = Vec::new();
    for layer in 0..layers {
        for schema in 0..width {
            let mut properties: Vec<_> = (0..6)
                .map(|p| format!(r#""s{p}":{{"type":"string"}}"#))
                .collect();
            if layer + 1 < layers {
                for c in 0..fan_out {
                    properties.push(format!(
                        r#""c{c}":{{"$ref":"{}"}}"#,
                        uri(layer + 1, (schema + c) % width)
                    ));
                }
            }
            let id = if with_id {
                format!(r#""$id":"{}","#, uri(layer, schema))
            } else {
                String::new()
            };
            schemas.push((
                uri(layer, schema),
                format!(
                    r#"{{{id}"type":"object","properties":{{{}}}}}"#,
                    properties.join(",")
                ),
            ));
        }
    }
    schemas
}
fn model_uri(layer: usize, schema: usize) -> String {
    format!("https://model.test/l{layer}/s{schema}")
}
// A value nested along the first reference of every layer; `wrong` puts a
// number where the deepest layer expects a string.
fn model_value(layers: usize, wrong: bool) -> JsonValue {
    let mut value = if wrong {
        r#"{"s0":7}"#.to_owned()
    } else {
        r#"{"s0":"x"}"#.to_owned()
    };
    for _ in 1..layers {
        value = format!(r#"{{"s1":"y","c0":{value}}}"#);
    }
    JsonValue::parse(value).unwrap()
}
fn decides(contract: &PreparedValueContract) {
    assert!(matches!(
        contract.validate(&JsonValue::parse("7").unwrap()),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        contract.validate(&JsonValue::parse("\"7\"").unwrap()),
        ValueOutcome::Fails { .. }
    ));
}
fn decides_model(contract: &PreparedValueContract, layers: usize) {
    assert!(matches!(
        contract.validate(&model_value(layers, false)),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        contract.validate(&model_value(layers, true)),
        ValueOutcome::Fails { .. }
    ));
}

// A thread stack far smaller than one frame per compiled link would need.
const SMALL_STACK: usize = 512 * 1024;
#[test]
fn reference_chains_prepare_and_release_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(|| {
            let contract = prepare(&chain(5_000)).unwrap();
            drop(contract);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn chains_the_default_stack_holds_decide() {
    decides(&prepare(&chain(500)).unwrap());
    decides(&prepare(&chain(1_000)).unwrap());
    decides(&prepare(&resource_chain(100)).unwrap());
    // Longer chains still prepare, though evaluation cannot follow them within
    // the default stack: each link nests one evaluation deeper.
    let contract = prepare(&chain(1_500)).unwrap();
    let ValueOutcome::NoVerdict { detail } = contract.validate(&JsonValue::parse("7").unwrap())
    else {
        panic!("evaluation follows no 1,500-link chain under default limits")
    };
    assert_eq!(detail.code, "evaluation-depth-limit");
}

#[test]
fn resource_diamonds_compile_each_level_once() {
    // Compiled once per path through resources, 24 levels would compile the
    // last one 2^24 times.
    decides(&prepare(&diamonds(4)).unwrap());
    decides(&prepare(&diamonds(12)).unwrap());
    // Evaluation still follows every path; an application that caps its work
    // gets no verdict instead.
    let capped = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: Some(1_000_000),
        ..Limits::default()
    });
    let text = diamonds(24);
    let proof = Document::parse(&text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let contract = proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(&capped)
        .unwrap();
    let ValueOutcome::NoVerdict { detail } = contract.validate(&JsonValue::parse("7").unwrap())
    else {
        panic!("evaluation follows no 2^24 paths within the cap")
    };
    assert_eq!(detail.code, "evaluation-work-limit");
}

#[test]
fn layered_models_across_resources_compile_each_schema_once() {
    // Supplied as separate resources, or as `$id` resources in the document,
    // a layered model reaches its deepest schemas along hundreds of paths.
    let (layers, fan_out, width) = (7, 3, 6);
    let supplied = resources(layered(layers, fan_out, width, &model_uri, false));
    let input = format!(r#"{{"$ref":"{}"}}"#, model_uri(0, 0));
    let contract = prepare_with(&document(&[], &input), &supplied).unwrap();
    decides_model(&contract, layers);

    let supplied = resources(layered(10, 2, 4, &model_uri, false));
    let contract = prepare_with(&document(&[], &input), &supplied).unwrap();
    decides_model(&contract, 10);

    let contained: Vec<_> = layered(layers, fan_out, width, &model_uri, true)
        .into_iter()
        .enumerate()
        .map(|(i, (_, schema))| format!(r#""m{i}":{schema}"#))
        .collect();
    let contract = prepare(&document(&contained, &input)).unwrap();
    decides_model(&contract, layers);
}

#[test]
fn dynamic_anchors_resolve_per_scope() {
    // One list resource, its items resolved through a dynamic anchor that two
    // resources referencing it override: the list is specialized per scope.
    let supplied = resources(vec![
        (
            "https://lists.test/list".into(),
            r##"{"type":"array","items":{"$dynamicRef":"#item"},"$defs":{"item":{"$dynamicAnchor":"item"}}}"##.into(),
        ),
        (
            "https://lists.test/integers".into(),
            r#"{"$ref":"list","$defs":{"item":{"$dynamicAnchor":"item","type":"integer"}}}"#.into(),
        ),
        (
            "https://lists.test/strings".into(),
            r#"{"$ref":"list","$defs":{"item":{"$dynamicAnchor":"item","type":"string"}}}"#.into(),
        ),
    ]);
    let input = r#"{"properties":{"i":{"$ref":"https://lists.test/integers"},"s":{"$ref":"https://lists.test/strings"},"any":{"$ref":"https://lists.test/list"}}}"#;
    let contract = prepare_with(&document(&[], input), &supplied).unwrap();
    for (value, satisfies) in [
        (r#"{"i":[1,2],"s":["x"],"any":[1,"x",null]}"#, true),
        (r#"{"i":["x"]}"#, false),
        (r#"{"s":[1]}"#, false),
        (r#"{"any":[{}],"s":[],"i":[]}"#, true),
    ] {
        let outcome = contract.validate(&JsonValue::parse(value).unwrap());
        if satisfies {
            assert!(matches!(outcome, ValueOutcome::Satisfies), "{value}");
        } else {
            assert!(matches!(outcome, ValueOutcome::Fails { .. }), "{value}");
        }
    }
    // Each path through the diamonds enters the same resources, so one
    // dynamic context reaches the leaf: 24 levels prepare as 4 do.
    decides(&prepare(&dynamic_diamonds(4)).unwrap());
    assert!(prepare(&dynamic_diamonds(24)).is_ok());
}

// `levels` levels, each a resource whose two branches reference the next level:
// the first branch also declares the dynamic anchor `n<level>`, the second does
// not.
// The last level resolves every name dynamically, so each of the 2^levels
// paths reaches it in a dynamic context of its own.
fn dynamic_choices(levels: usize) -> String {
    let uri = |name: String| format!("https://choices.test/{name}");
    let mut schemas = Vec::new();
    for i in 0..levels {
        let next = uri(format!("l{}", i + 1));
        schemas.push(format!(
            r#""l{i}":{{"$id":"{}","anyOf":[{{"$ref":"{}"}},{{"$ref":"{}"}}]}}"#,
            uri(format!("l{i}")),
            uri(format!("a{i}")),
            uri(format!("b{i}"))
        ));
        schemas.push(format!(
            r#""a{i}":{{"$id":"{}","$ref":"{next}","$defs":{{"n":{{"$dynamicAnchor":"n{i}","minimum":0}}}}}}"#,
            uri(format!("a{i}"))
        ));
        schemas.push(format!(
            r#""b{i}":{{"$id":"{}","$ref":"{next}"}}"#,
            uri(format!("b{i}"))
        ));
    }
    let anchors: Vec<_> = (0..levels)
        .map(|i| format!(r#""n{i}":{{"$dynamicAnchor":"n{i}"}}"#))
        .collect();
    let references: Vec<_> = (0..levels)
        .map(|i| format!(r##"{{"$dynamicRef":"#n{i}"}}"##))
        .collect();
    schemas.push(format!(
        r#""l{levels}":{{"$id":"{}","type":"integer","allOf":[{}],"$defs":{{{}}}}}"#,
        uri(format!("l{levels}")),
        references.join(","),
        anchors.join(",")
    ));
    document(&schemas, r##"{"$ref":"#/schemas/l0"}"##)
}
#[test]
fn dynamic_specialization_is_bounded() {
    decides(&prepare(&dynamic_choices(8)).unwrap());
    let refused = prepare(&dynamic_choices(17)).unwrap_err();
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
    assert_eq!(refused.code, "program-specialization-limit");
    assert!(refused.location.is_none());
}

#[test]
fn the_program_budget_scales_dynamic_specialization() {
    let within = |levels: usize, program_bytes: usize| {
        prepare_within(
            &dynamic_choices(levels),
            &ResourceSet::default(),
            Limits {
                program_bytes,
                ..Limits::default()
            },
        )
    };
    // 256 dynamic contexts reach the last level of 8, whose copies take about
    // 6.8 MiB: a smaller budget refuses them, and says what to raise.
    let refused = within(8, 4 << 20).unwrap_err();
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
    assert_eq!(refused.code, "program-specialization-limit");
    assert!(
        refused.message.ends_with(
            "by more than its 4 MiB program_bytes; raise program_bytes (programBytes in TypeScript)"
        ),
        "{}",
        refused.message
    );
    decides(&within(8, 8 << 20).unwrap());
    // Twice the contexts need about twice the budget.
    assert_eq!(
        within(9, 8 << 20).unwrap_err().code,
        "program-specialization-limit"
    );
    decides(&within(9, 16 << 20).unwrap());
    // No budget refuses a contract that specializes nothing.
    let unspecialized = prepare_within(
        &chain(3),
        &ResourceSet::default(),
        Limits {
            program_bytes: 0,
            ..Limits::default()
        },
    );
    decides(&unspecialized.unwrap());
}

#[test]
fn a_specialization_refusal_states_its_arithmetic() {
    let refused = prepare_within(
        &dynamic_choices(8),
        &ResourceSet::default(),
        Limits {
            program_bytes: 4 << 20,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        refused.message,
        "this value contract's dynamic references specialize 3352 schemas, taking 4 MiB, and 304 \
         dynamic contexts holding 1740 entries, taking 32 KiB, before preparation stops: \
         together they pass its projected schemas' 76 KiB by more than its 4 MiB program_bytes; \
         raise program_bytes (programBytes in TypeScript)"
    );
}

#[test]
fn long_required_lists_compile_once_per_specialization() {
    // A shared resource requiring 20,000 names, reached through 1,000
    // resources, compiles once: with no dynamic anchor, and with one that
    // every path resolves alike.
    for dynamic in [false, true] {
        let contract = prepare(&required_fan(1_000, 20_000, dynamic)).unwrap();
        assert!(matches!(
            contract.validate(&JsonValue::parse(r#"{"p0":{}}"#).unwrap()),
            ValueOutcome::Fails { .. }
        ));
    }
}

// Generated contracts whose `unevaluatedProperties` and `unevaluatedItems` walks
// cross resources through `patternProperties`, `allOf`, `oneOf` and conditionals.
// Compiled once per path through resources they took seconds to over a minute;
// compiled once per location they prepare in milliseconds.
#[derive(serde::Deserialize)]
struct GeneratedCase {
    id: String,
    // Raw text keeps each schema's keyword order, which decides what compiles first.
    document: Box<serde_json::value::RawValue>,
    resources: Vec<GeneratedResource>,
    values: Vec<Box<serde_json::value::RawValue>>,
    expected: Vec<String>,
}
#[derive(serde::Deserialize)]
struct GeneratedResource {
    uri: String,
    schema: Box<serde_json::value::RawValue>,
}
fn generated_cases() -> Vec<GeneratedCase> {
    serde_json::from_str(include_str!("fixtures/preparation-generated-walks.json")).unwrap()
}
fn generated_resources(case: &GeneratedCase) -> ResourceSet {
    resources(
        case.resources
            .iter()
            .map(|r| (r.uri.clone(), r.schema.get().to_owned()))
            .collect(),
    )
}

#[test]
fn generated_walks_across_resources_compile_once() {
    for case in generated_cases() {
        let contract = prepare_with(case.document.get(), &generated_resources(&case))
            .unwrap_or_else(|detail| panic!("{}: {detail:?}", case.id));
        for (value, expected) in case.values.iter().zip(&case.expected) {
            let value = JsonValue::parse(value.get()).unwrap();
            let label = format!("{} {}", case.id, value.text());
            for outcome in [contract.validate(&value), contract.explain(&value)] {
                let verdict = match outcome {
                    ValueOutcome::Satisfies => "S",
                    ValueOutcome::Fails { .. } => "F",
                    _ => "neither",
                };
                assert_eq!(verdict, expected, "{label}");
            }
        }
    }
}

#[test]
fn generated_walks_reaching_a_dynamic_anchor_compile_once_too() {
    // A property holding a 2020-12 schema reaches the meta-schema's dynamic
    // anchor; the walks still prepare and decide as before.
    for case in generated_cases() {
        let mut document: serde_json::Value = serde_json::from_str(case.document.get()).unwrap();
        let input = document["operations"]["op"]["input"].take();
        document["operations"]["op"]["input"] = serde_json::json!({"allOf": [
            input,
            {"properties": {"schema": {"$ref": "https://json-schema.org/draft/2020-12/schema"}}}
        ]});
        let contract = prepare_with(&document.to_string(), &generated_resources(&case))
            .unwrap_or_else(|detail| panic!("{}: {detail:?}", case.id));
        for (value, expected) in case.values.iter().zip(&case.expected) {
            let value = JsonValue::parse(value.get()).unwrap();
            let verdict = match contract.validate(&value) {
                ValueOutcome::Satisfies => "S",
                ValueOutcome::Fails { .. } => "F",
                _ => "neither",
            };
            assert_eq!(verdict, expected, "{} {}", case.id, value.text());
        }
    }
}

// Prepare the input contract of a document admitted under `limits`.
fn prepare_admitted(text: &str, limits: JsonLimits) -> Result<PreparedValueContract, NoVerdict> {
    validate_document_with_limits(text, limits)
        .unwrap()
        .conformant()
        .expect("conformant document")
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
}

#[test]
fn a_projection_keeps_its_own_bounds_whatever_the_document_was_admitted_under() {
    // Each empty `allOf` member projects to a reference and a definition, so
    // the projection is many times the document; admitting the document at its
    // own size or node count does not bound it.
    let text = document(
        &[],
        &format!(r#"{{"allOf":[{}]}}"#, vec!["{}"; 20_000].join(",")),
    );
    // The root, three members and their values, `op`, `input`, `allOf` and the
    // members: 20,013 nodes, the document's own count.
    let nodes = 20_013;
    let fewer = JsonLimits {
        max_nodes: nodes - 1,
        ..JsonLimits::default()
    };
    assert!(JsonValue::parse_with_limits(&text, fewer).is_err());
    for limits in [
        JsonLimits {
            max_bytes: text.len(),
            ..JsonLimits::default()
        },
        JsonLimits {
            max_nodes: nodes,
            ..JsonLimits::default()
        },
        JsonLimits::default(),
    ] {
        assert!(
            validate_document_with_limits(&text, limits)
                .unwrap()
                .conformant()
                .is_some()
        );
        assert!(prepare_admitted(&text, limits).is_ok(), "{limits:?}");
    }
    // Supplied resources together past the document's byte limit prepare.
    let pad = "a".repeat(17 << 20);
    let resource = |uri: &str, body: &str| SchemaResource {
        uri: uri.into(),
        document: JsonValue::parse(format!(
            r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema",{body},"x-pad":"{pad}"}}"#
        ))
        .unwrap(),
    };
    let supplied = ResourceSet::new([
        resource(
            "https://e.test/a",
            r#""allOf":[{"$ref":"https://e.test/b"}]"#,
        ),
        resource("https://e.test/b", r#""type":"string""#),
    ])
    .unwrap();
    let small = document(&[], r#"{"$ref":"https://e.test/a"}"#);
    let contract = prepare_with(&small, &supplied).unwrap();
    assert!(matches!(
        contract.validate_json(r#""x""#).unwrap(),
        ValueOutcome::Satisfies
    ));
    // A document admitted past the default byte limit prepares a contract whose
    // projection passes that limit, and decides a value.
    let default = JsonLimits::default().max_bytes;
    let literal = format!(r#""{}""#, "a".repeat(default));
    let large = document(&[], &format!(r#"{{"const":{literal}}}"#));
    let raised = JsonLimits {
        max_bytes: 2 * default,
        ..JsonLimits::default()
    };
    let contract = prepare_admitted(&large, raised).unwrap();
    let value = JsonValue::parse_with_limits(&literal, raised).unwrap();
    assert!(matches!(contract.validate(&value), ValueOutcome::Satisfies));
}
