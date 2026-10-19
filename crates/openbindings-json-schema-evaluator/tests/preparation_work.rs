//! Preparation spends a bounded amount of library compilation work for each
//! projected schema definition. Long reference chains, whose compilation grows
//! faster than the schema itself, refuse with a limit instead of running on. A
//! schema reached along many reference paths through resources compiles once
//! unless a dynamic anchor can make those paths resolve differently.
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

#[test]
fn reference_chains_refuse_from_an_exact_length() {
    // Compiling a chain costs the library work quadratic in its length, and a
    // chain of `$id` resources costs what a same-document chain does. These
    // boundaries pin the accounting; the Wasm lane pins the same ones.
    assert!(prepare(&chain(1_788)).is_ok());
    assert_work_limit(prepare(&chain(1_789)).unwrap_err());
    assert!(prepare(&resource_chain(1_788)).is_ok());
    assert_work_limit(prepare(&resource_chain(1_789)).unwrap_err());
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
    // last one 2^24 times; 12 already spend more than the allowance.
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
    // A reachable dynamic anchor keeps every schema compiled per path, so the
    // diamond ladder refuses from the same depth it would without caching.
    decides(&prepare(&dynamic_diamonds(8)).unwrap());
    assert_work_limit(prepare(&dynamic_diamonds(9)).unwrap_err());
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

#[test]
fn library_builds_after_a_preparation_are_outside_its_compilation_scope() {
    // Same thread: a refused preparation leaves no spent scope behind, and a
    // preparation without dynamic anchors leaves no static statement behind.
    let dynamic = serde_json::json!({
        "$dynamicAnchor": "node",
        "properties": {"next": {"$dynamicRef": "#node"}, "n": {"type": "integer"}}
    });
    let instance = serde_json::json!({"next": {"n": "x"}});
    assert_work_limit(prepare(&chain(1_789)).unwrap_err());
    let plain = jsonschema::validator_for(&dynamic).unwrap();
    assert!(!plain.is_valid(&instance));
    decides(&prepare(&diamonds(4)).unwrap());
    let plain = jsonschema::validator_for(&dynamic).unwrap();
    assert!(!plain.is_valid(&instance));
    assert!(plain.is_valid(&serde_json::json!({"next": {"n": 1}})));
}
