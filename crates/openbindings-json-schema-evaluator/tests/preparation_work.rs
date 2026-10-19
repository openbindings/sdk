//! Preparation spends a bounded amount of library compilation work: a floor plus
//! a share for each projected schema definition. The library weighs what it does,
//! so long reference chains, schemas compiled once per path and long keyword lists
//! copied once per scope all refuse with a limit instead of running on. A schema
//! reached along many reference paths through resources compiles once unless a
//! dynamic anchor can make those paths resolve differently.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn prepare_with(text: &str, resources: &ResourceSet) -> Result<PreparedValueContract, NoVerdict> {
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
        .prepare_with(&DefaultEvaluator::new(), resources)
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
fn assert_work_limit(refused: NoVerdict) {
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
    assert_eq!(refused.code, "preparation-work-limit");
    assert!(refused.location.is_none());
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
// Compiling a chain costs the library work quadratic in its length: each deferred
// link copies the placeholders of every link before it. These boundaries pin the
// accounting; the Wasm lane pins the same ones.
#[test]
fn reference_chains_prepare_up_to_an_exact_length() {
    assert!(prepare(&chain(23_496)).is_ok());
}
#[test]
fn reference_chains_refuse_from_an_exact_length() {
    // The refused build has compiled most of the chain; it is released link by
    // link, as a prepared one is, so even a small stack suffices.
    let refused = std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(|| prepare(&chain(23_497)).unwrap_err())
        .unwrap()
        .join()
        .unwrap();
    assert_work_limit(refused);
}

#[test]
fn shorter_chains_prepare_within_the_allowance() {
    decides(&prepare(&chain(500)).unwrap());
    decides(&prepare(&resource_chain(100)).unwrap());
    // Longer chains still prepare, though evaluation cannot follow them under
    // the default evaluation limits: each link nests one evaluation deeper.
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
    // last one 2^24 times; 13 already spend more than the allowance.
    decides(&prepare(&diamonds(4)).unwrap());
    decides(&prepare(&diamonds(12)).unwrap());
    // Evaluation still follows every path, within the evaluation limits.
    let contract = prepare(&diamonds(24)).unwrap();
    let ValueOutcome::NoVerdict { detail } = contract.validate(&JsonValue::parse("7").unwrap())
    else {
        panic!("evaluation follows no 2^24 paths under default limits")
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
fn dynamic_anchors_keep_compilation_per_scope() {
    // One list resource, its items resolved through a dynamic anchor that two
    // resources referencing it override: the list compiles once per scope.
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
    // A reachable dynamic anchor keeps every schema compiled per path.
    decides(&prepare(&dynamic_diamonds(12)).unwrap());
}

#[test]
fn dynamic_resource_diamonds_refuse_from_an_exact_depth() {
    // Compiled per path, the diamond ladder refuses from the depth it would
    // without caching. The Wasm lane pins the same boundary.
    assert_work_limit(prepare(&dynamic_diamonds(13)).unwrap_err());
}

#[test]
fn long_required_lists_are_charged_per_compilation() {
    // Each compilation of the shared resource copies its 20,000 names, so a
    // contract that compiles it once per resource path refuses from an exact
    // number of paths, far fewer than its node count alone would allow. The
    // Wasm lane pins the same boundary.
    let prepared = prepare(&required_fan(103, 20_000, true)).unwrap();
    assert!(matches!(
        prepared.validate(&JsonValue::parse(r#"{"p0":{}}"#).unwrap()),
        ValueOutcome::Fails { .. }
    ));
    assert_work_limit(prepare(&required_fan(104, 20_000, true)).unwrap_err());
    // Without a dynamic anchor the list compiles once, for any number of paths.
    assert!(prepare(&required_fan(1_000, 20_000, false)).is_ok());
}

#[test]
fn compilation_charges_each_kind_of_work() {
    use jsonschema::ob_work::{Stop, compiling};
    use serde_json::json;
    let long = "x".repeat(100);
    let chain: serde_json::Map<_, _> = (0..30)
        .map(|i| {
            (
                format!("d{i}"),
                json!({"$ref": format!("#/$defs/d{}", i + 1)}),
            )
        })
        .chain([("d30".to_owned(), json!({"type": "integer"}))])
        .collect();
    // Each build needs exactly these units: a node 830, plus 8 per byte of its
    // location; a reference 750; each property name copied from `required` or
    // `dependentRequired` 110, plus 2 per byte; each `dependentRequired` entry a
    // node and 2 per byte of its name; each pattern byte 1; each placeholder a
    // deferred reference copies 10; and, with a dynamic anchor, 30 per dynamic
    // scope entry keying a node and 440 per entry searched for the anchor.
    let cases = [
        (true, json!({"type": "integer"}), 830),
        // A child at "/properties/name": 830 + 16 * 8.
        (
            true,
            json!({"properties": {"name": {"type": "string"}}}),
            1_788,
        ),
        (true, json!({"properties": {long.clone(): {}}}), 2_556),
        // A reference to a node at "/$defs/a": 830 + 750 + 830 + 8 * 8.
        (
            true,
            json!({"$defs": {"a": {"type": "integer"}}, "$ref": "#/$defs/a"}),
            2_474,
        ),
        (true, json!({"required": ["a", "b"]}), 1_054),
        (
            true,
            json!({"required": ["name", "kind", "size", "tags"]}),
            1_302,
        ),
        (true, json!({"required": [long]}), 1_140),
        (true, json!({"dependentRequired": {"a": ["b", "c"]}}), 1_886),
        (true, json!({"pattern": "^[a-z]+$"}), 838),
        (true, json!({"$defs": chain, "$ref": "#/$defs/d0"}), 52_720),
        (
            false,
            json!({
                "$dynamicAnchor": "node",
                "properties": {"next": {"$dynamicRef": "#node"}, "n": {"type": "integer"}}
            }),
            7_914,
        ),
    ];
    for (static_anchors, schema, units) in cases {
        let built = compiling(units, static_anchors, || jsonschema::validator_for(&schema));
        assert!(built.unwrap().is_ok(), "{schema}");
        let short = compiling(units - 1, static_anchors, || {
            jsonschema::validator_for(&schema)
        });
        assert_eq!(short.err(), Some(Stop::Work), "{schema}");
    }
}

#[test]
fn builds_that_state_static_anchors_refuse_dynamic_resolution() {
    use jsonschema::ob_work::compiling;
    let dynamic = serde_json::json!({
        "$dynamicAnchor": "node",
        "properties": {"next": {"$dynamicRef": "#node"}, "n": {"type": "integer"}}
    });
    let recursive = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2019-09/schema",
        "$recursiveAnchor": true,
        "properties": {"next": {"$recursiveRef": "#"}, "n": {"type": "integer"}}
    });
    let instance = serde_json::json!({"next": {"n": "x"}});
    for schema in [&dynamic, &recursive] {
        let refused = compiling(1 << 20, true, || jsonschema::validator_for(schema)).unwrap();
        assert!(refused.is_err());
        let built = compiling(1 << 20, false, || jsonschema::validator_for(schema)).unwrap();
        assert!(!built.unwrap().is_valid(&instance));
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
fn generated_walks_with_a_dynamic_anchor_refuse_at_preparation() {
    // A property holding a 2020-12 schema reaches the meta-schema's dynamic
    // anchor, so the same walks compile once per path again; the allowance
    // refuses them instead.
    let case = generated_cases().pop().unwrap();
    let mut document: serde_json::Value = serde_json::from_str(case.document.get()).unwrap();
    let input = document["operations"]["op"]["input"].take();
    document["operations"]["op"]["input"] = serde_json::json!({"allOf": [
        input,
        {"properties": {"schema": {"$ref": "https://json-schema.org/draft/2020-12/schema"}}}
    ]});
    let refused = prepare_with(&document.to_string(), &generated_resources(&case)).unwrap_err();
    assert_work_limit(refused);
}

// Build `root` against `resources` inside a compilation scope stating, or not,
// that no dynamic anchor is reachable; `None` builds outside any scope.
fn build_in_scope(
    resources: &[(&str, serde_json::Value)],
    static_anchors: Option<bool>,
) -> Option<jsonschema::Validator> {
    let mut registry = jsonschema::Registry::new();
    for (uri, schema) in resources {
        registry = registry.add(*uri, schema.clone()).unwrap();
    }
    let registry = registry.prepare().unwrap();
    let root = jsonschema::Uri::parse(resources[0].0.to_owned()).unwrap();
    let root = registry.resolver(root).lookup("").unwrap();
    let build = || {
        jsonschema::options()
            .with_registry(&registry)
            .build(root.contents())
    };
    match static_anchors {
        None => Some(build().unwrap()),
        Some(stated) => jsonschema::ob_work::compiling(1 << 30, stated, build)
            .unwrap()
            .ok(),
    }
}

#[test]
fn unevaluated_walks_in_static_builds_refuse_dynamic_resolution() {
    // An `unevaluatedProperties` or `unevaluatedItems` walk resolves the
    // references of the schemas it passes through itself, even keywords the
    // schema's draft does not compile: `$dynamicRef` in a 2019-09 schema and
    // `$recursiveRef` in a 2020-12 one. Reached from `a`, which declares the
    // anchor, the shared schema `x` resolves to `a`; reached from `b`, it does
    // not. A build wrongly stated static refuses instead of compiling `x` once
    // for both paths; built per scope, it decides as the library does outside any
    // compilation scope.
    use serde_json::json;
    let uri = |name: &str| format!("https://walks.test/{name}");
    let (e, a, b, x, y) = (uri("e"), uri("a"), uri("b"), uri("x"), uri("y"));
    let anchor_y = format!("{y}#n");
    let draft_2019 = "https://json-schema.org/draft/2019-09/schema";
    let shapes = [
        // `$dynamicRef` in a 2019-09 schema.
        (
            json!({"$dynamicAnchor": "n", "properties": {"a": true}, "$ref": x}),
            json!({"properties": {"b": true}, "$ref": x}),
            json!({"$schema": draft_2019, "$dynamicRef": anchor_y, "unevaluatedProperties": false}),
            json!({"a": 1}),
        ),
        (
            json!({"$dynamicAnchor": "n", "prefixItems": [true], "$ref": x}),
            json!({"$ref": x}),
            json!({"$schema": draft_2019, "$dynamicRef": anchor_y, "unevaluatedItems": false}),
            json!([1]),
        ),
        // A named `$ref` in a schema only the walk enters, through a pointer
        // `$dynamicRef` that 2019-09 does not compile.
        (
            json!({"$dynamicAnchor": "n", "properties": {"a": true}, "$ref": x}),
            json!({"$ref": x}),
            json!({
                "$schema": draft_2019,
                "$dynamicRef": "#/$defs/t",
                "$defs": {"t": {"$ref": anchor_y}},
                "unevaluatedProperties": false
            }),
            json!({"a": 1}),
        ),
        (
            json!({"$dynamicAnchor": "n", "prefixItems": [true], "$ref": x}),
            json!({"$ref": x}),
            json!({
                "$schema": draft_2019,
                "$dynamicRef": "#/$defs/t",
                "$defs": {"t": {"$ref": anchor_y}},
                "unevaluatedItems": false
            }),
            json!([1]),
        ),
        // `$recursiveRef` in a 2020-12 schema.
        (
            json!({"$recursiveAnchor": true, "properties": {"a": true}, "$ref": x}),
            json!({"properties": {"b": true}, "$ref": x}),
            json!({"$recursiveAnchor": true, "$recursiveRef": "#", "unevaluatedProperties": false}),
            json!({"a": 1}),
        ),
        // The same, `x` reached through a property the walk does not enter.
        (
            json!({"$recursiveAnchor": true, "properties": {"a": true, "z": {"$ref": x}}}),
            json!({"properties": {"z": {"$ref": x}}}),
            json!({"$recursiveAnchor": true, "$recursiveRef": "#", "unevaluatedProperties": false}),
            json!({"z": {"a": 1}}),
        ),
        (
            json!({"$recursiveAnchor": true, "prefixItems": [true], "properties": {"z": {"$ref": x}}}),
            json!({"properties": {"z": {"$ref": x}}}),
            json!({"$recursiveAnchor": true, "$recursiveRef": "#", "unevaluatedItems": false}),
            json!({"z": [1]}),
        ),
    ];
    for (a_schema, b_schema, x_schema, value) in &shapes {
        // Either path may compile `x` first: properties compile in name order.
        for (first, second) in [(&a, &b), (&b, &a)] {
            let entry = json!({"properties": {"p": {"$ref": first}, "q": {"$ref": second}}});
            let resources = [
                (e.as_str(), entry),
                (a.as_str(), a_schema.clone()),
                (b.as_str(), b_schema.clone()),
                (x.as_str(), x_schema.clone()),
                (y.as_str(), json!({"$dynamicAnchor": "n"})),
            ];
            assert!(
                build_in_scope(&resources, Some(true)).is_none(),
                "{x_schema} from {first}"
            );
            let plain = build_in_scope(&resources, None).unwrap();
            let scoped = build_in_scope(&resources, Some(false)).unwrap();
            for value in [json!({"p": value}), json!({"q": value})] {
                assert_eq!(scoped.is_valid(&value), plain.is_valid(&value), "{value}");
            }
        }
    }
}

#[test]
fn library_builds_after_a_preparation_are_outside_its_compilation_scope() {
    // Same thread: a refused preparation leaves no spent scope behind, and a
    // preparation without dynamic anchors leaves no static statement behind.
    let dynamic = serde_json::json!({
        "$dynamicAnchor": "node",
        "properties": {"next": {"$dynamicRef": "#node"}, "n": {"type": "integer"}}
    });
    let instance = serde_json::json!({"next": {"n": "x"}});
    assert_work_limit(prepare(&required_fan(104, 20_000, true)).unwrap_err());
    let plain = jsonschema::validator_for(&dynamic).unwrap();
    assert!(!plain.is_valid(&instance));
    decides(&prepare(&diamonds(4)).unwrap());
    let plain = jsonschema::validator_for(&dynamic).unwrap();
    assert!(!plain.is_valid(&instance));
    assert!(plain.is_valid(&serde_json::json!({"next": {"n": 1}})));
}
