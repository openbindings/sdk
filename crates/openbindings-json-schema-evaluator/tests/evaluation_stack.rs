//! The evaluation bounds the evaluator sizes from `Limits::stack_bytes` protect a
//! thread of that size: a value as deep as the evaluator admits is checked and
//! explained on it, and a deeper one is refused, before evaluation by the static
//! bound or, for a contract holding an unevaluated keyword, by the stack guard as
//! it evaluates. The contracts recurse through every applicator family,
//! including the unevaluated chains, and are prepared on the same thread; a chain
//! longer than the walk limit refuses at preparation instead. A larger stack
//! admits deeper values and longer walks, and a smaller one refuses them sooner.
//! The only test in this executable, so nothing else shares its thread.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

// Recursive contracts, each with a value `depth` levels deep. Failing values make
// explanations build the library's error from the deepest level.
fn shape(name: &str, depth: usize) -> (String, String) {
    let nest = |open: &str, leaf: &str, close: &str| {
        format!("{}{leaf}{}", open.repeat(depth), close.repeat(depth))
    };
    if let Some(generated) = chain(name, depth) {
        return generated;
    }
    let (schema, value) = match name {
        "items" => (
            r##"{"type":"array","items":{"$ref":"#"}}"##,
            nest("[", "", "]"),
        ),
        "items-fail" => (
            r##"{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#"}}]}"##,
            nest("[", r#""x""#, "]"),
        ),
        "properties-fail" => (
            r##"{"type":"object","properties":{"a":{"$ref":"#"}},"required":["a"]}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "pattern-properties-fail" => (
            r##"{"type":"object","patternProperties":{"^a":{"$ref":"#"}},"minProperties":1}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "unevaluated-properties-fail" => (
            r##"{"type":"object","properties":{"b":true},"unevaluatedProperties":{"$ref":"#"},"minProperties":1}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "unevaluated-items-fail" => (
            r##"{"type":"array","prefixItems":[true],"unevaluatedItems":{"$ref":"#"},"minItems":2}"##,
            nest("[0,", "[0]", "]"),
        ),
        "one-of-fail" => (
            r##"{"oneOf":[{"type":"string"},{"type":"array","items":{"$ref":"#"}}]}"##,
            nest("[", "0", "]"),
        ),
        "names-fail" => (
            r##"{"type":"object","propertyNames":{"maxLength":3},"additionalProperties":{"$ref":"#"},"minProperties":1}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        // An unevaluated keyword whose annotation sources the library
        // evaluates rather than reads, recursing through its own subschema.
        "unevaluated-any-of" => (
            r##"{"anyOf":[{}],"unevaluatedProperties":{"$ref":"#"}}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "unevaluated-one-of" => (
            r##"{"oneOf":[{}],"unevaluatedProperties":{"$ref":"#"}}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "unevaluated-if" => (
            r##"{"if":{},"unevaluatedProperties":{"$ref":"#"}}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "unevaluated-dependent" => (
            r##"{"dependentSchemas":{"a":{}},"unevaluatedProperties":{"$ref":"#"}}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "unevaluated-dynamic" => (
            r##"{"$dynamicAnchor":"n","anyOf":[{}],"unevaluatedProperties":{"$dynamicRef":"#n"}}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        "unevaluated-items-any-of" => (
            r##"{"anyOf":[{}],"unevaluatedItems":{"$ref":"#"}}"##,
            nest("[", "[]", "]"),
        ),
        "unevaluated-items-contains" => (
            r##"{"contains":{},"minContains":0,"anyOf":[{}],"unevaluatedItems":{"$ref":"#"}}"##,
            nest("[", "[]", "]"),
        ),
        // A holder whose walk follows eight `allOf` levels to the recursion.
        "unevaluated-walk" => (
            r##"{"allOf":[{"$ref":"#/$defs/a1"}],"unevaluatedProperties":false,"$defs":{"a1":{"allOf":[{"$ref":"#/$defs/a2"}]},"a2":{"allOf":[{"$ref":"#/$defs/a3"}]},"a3":{"allOf":[{"$ref":"#/$defs/a4"}]},"a4":{"allOf":[{"$ref":"#/$defs/a5"}]},"a5":{"allOf":[{"$ref":"#/$defs/a6"}]},"a6":{"allOf":[{"$ref":"#/$defs/a7"}]},"a7":{"allOf":[{"$ref":"#/$defs/a8"}]},"a8":{"properties":{"a":{"$ref":"#"}}}}}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        // Holders nested in each other's annotation sources.
        "nested-holders" => (
            r##"{"allOf":[{"$ref":"#/$defs/a"}],"unevaluatedProperties":false,"$defs":{"a":{"anyOf":[{"$ref":"#/$defs/b"}],"unevaluatedProperties":{"$ref":"#"}},"b":{"oneOf":[{}],"unevaluatedProperties":true}}}"##,
            nest(r#"{"a":"#, "{}", "}"),
        ),
        _ => unreachable!(),
    };
    (schema.to_owned(), value)
}
// An unevaluated keyword whose annotation sources run through a chain of `k`
// schemas (`$ref`, `if`/`then`, `dependentSchemas`, `allOf`) to an `anyOf`
// whose branch recurses: `chain-ref-16`, `chain-ref-fail-16` (a failing leaf),
// `chain-if-16`, `chain-dependent-16`, `chain-all-of-16`, `chain-items-16`.
fn chain(name: &str, depth: usize) -> Option<(String, String)> {
    let rest = name.strip_prefix("chain-")?;
    let (kind, k) = rest.rsplit_once('-')?;
    let k: usize = k.parse().ok()?;
    let link = |i: usize| -> serde_json::Value {
        let next = serde_json::json!({"$ref": format!("#/$defs/c{}", i + 1)});
        match kind {
            "ref" | "ref-fail" | "items" => next,
            "if" => serde_json::json!({"if": true, "then": next}),
            "dependent" => serde_json::json!({"dependentSchemas": {"a": next}}),
            "all-of" => serde_json::json!({"allOf": [next]}),
            _ => unreachable!("{kind}"),
        }
    };
    let mut defs = serde_json::Map::new();
    for i in 1..k {
        defs.insert(format!("c{i}"), link(i));
    }
    let (leaf_schema, holder, open, leaf, close) = match kind {
        "ref-fail" => (
            serde_json::json!({"anyOf": [{"type": "object", "maxProperties": 0}, {"properties": {"a": {"$ref": "#"}}, "required": ["a"]}]}),
            "unevaluatedProperties",
            r#"{"a":"#,
            "1",
            "}",
        ),
        "items" => (
            serde_json::json!({"anyOf": [{}, {"prefixItems": [{"$ref": "#"}]}]}),
            "unevaluatedItems",
            "[",
            "[]",
            "]",
        ),
        _ => (
            serde_json::json!({"anyOf": [{}, {"properties": {"a": {"$ref": "#"}}}]}),
            "unevaluatedProperties",
            r#"{"a":"#,
            "{}",
            "}",
        ),
    };
    defs.insert(format!("c{k}"), leaf_schema);
    let mut schema = match link(0) {
        serde_json::Value::Object(root) => root,
        _ => unreachable!(),
    };
    schema.insert(holder.into(), false.into());
    schema.insert("$defs".into(), defs.into());
    Some((
        serde_json::Value::Object(schema).to_string(),
        format!("{}{leaf}{}", open.repeat(depth), close.repeat(depth)),
    ))
}
// Each shape and the least depth it must admit; a shape with none may instead
// refuse at preparation, when its walk runs longer than the evaluator admits.
const SHAPES: [(&str, usize); 39] = [
    ("items", 100),
    ("items-fail", 100),
    ("properties-fail", 100),
    ("pattern-properties-fail", 100),
    ("unevaluated-properties-fail", 100),
    ("unevaluated-items-fail", 100),
    ("one-of-fail", 100),
    ("names-fail", 100),
    ("unevaluated-any-of", 100),
    ("unevaluated-one-of", 100),
    ("unevaluated-if", 100),
    ("unevaluated-dependent", 100),
    ("unevaluated-dynamic", 100),
    ("unevaluated-items-any-of", 100),
    ("unevaluated-items-contains", 100),
    ("unevaluated-walk", 20),
    ("nested-holders", 20),
    ("chain-ref-8", 1),
    ("chain-ref-16", 1),
    ("chain-ref-24", 1),
    ("chain-ref-30", 0),
    ("chain-ref-fail-8", 1),
    ("chain-ref-fail-16", 1),
    ("chain-ref-fail-24", 1),
    ("chain-ref-fail-30", 0),
    ("chain-if-6", 1),
    ("chain-if-16", 0),
    ("chain-if-30", 0),
    ("chain-dependent-6", 1),
    ("chain-dependent-16", 0),
    ("chain-dependent-30", 0),
    ("chain-all-of-7", 1),
    ("chain-all-of-16", 0),
    ("chain-all-of-30", 0),
    ("chain-items-8", 1),
    ("chain-items-16", 1),
    ("chain-items-24", 1),
    ("chain-items-30", 0),
    ("chain-ref-80", 0),
];
fn contract(
    schema: &str,
    evaluator: &DefaultEvaluator,
) -> Result<PreparedValueContract, NoVerdict> {
    let resources = ResourceSet::new([SchemaResource {
        uri: "https://stack.test/s".into(),
        document: JsonValue::parse(schema).unwrap(),
    }])
    .unwrap();
    let proof = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://stack.test/s"}}}}"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(evaluator, &resources)
}
fn refused(outcome: &ValueOutcome) -> bool {
    matches!(outcome, ValueOutcome::NoVerdict { detail } if detail.code == "evaluation-depth-limit")
}
// On a thread of `stack` bytes, with the evaluator sized for it: the deepest
// value of each shape the evaluator admits, checked and explained there, with
// one level more refused; `None` for a shape whose walk refuses preparation.
fn admitted(stack: usize) -> Vec<Option<usize>> {
    let evaluator = if stack == 256 * 1024 {
        DefaultEvaluator::new()
    } else {
        DefaultEvaluator::with_limits(Limits {
            stack_bytes: stack,
            ..Limits::default()
        })
    };
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let mut admitted = Vec::new();
            for (name, _) in SHAPES {
                let (schema, _) = shape(name, 0);
                let contract = match contract(&schema, &evaluator) {
                    Ok(contract) => contract,
                    Err(detail) => {
                        assert_eq!(detail.code, "unevaluated-walk-limit", "{stack}: {name}");
                        admitted.push(None);
                        continue;
                    }
                };
                // The deepest value the bound admits; core admits values nested
                // at most 10,000 levels.
                let (mut deepest, mut refused_at) = (0, 9_990);
                while refused_at - deepest > 1 {
                    let depth = (deepest + refused_at) / 2;
                    let value = JsonValue::parse(shape(name, depth).1).unwrap();
                    if refused(&contract.validate(&value)) {
                        refused_at = depth;
                    } else {
                        deepest = depth;
                    }
                }
                let value = JsonValue::parse(shape(name, deepest).1).unwrap();
                let checked = contract.validate(&value);
                let explained = contract.explain(&value);
                assert!(
                    !refused(&checked) && !refused(&explained),
                    "{stack}: {name}"
                );
                assert_eq!(
                    matches!(checked, ValueOutcome::Satisfies),
                    matches!(explained, ValueOutcome::Satisfies),
                    "{stack}: {name}"
                );
                let deeper = JsonValue::parse(shape(name, refused_at).1).unwrap();
                assert!(refused(&contract.validate(&deeper)), "{stack}: {name}");
                assert!(refused(&contract.explain(&deeper)), "{stack}: {name}");
                admitted.push(Some(deepest));
            }
            admitted
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "unoptimized evaluation frames exceed 256 KiB at the default depth; verified in release"
)]
fn each_stack_admits_only_values_it_evaluates() {
    let default = admitted(256 * 1024);
    for ((name, least), deepest) in SHAPES.into_iter().zip(&default) {
        match deepest {
            Some(deepest) => assert!(*deepest >= least, "{name}: {deepest}"),
            None => assert_eq!(least, 0, "{name}"),
        }
    }
    // A smaller stack refuses sooner, and every shape the default admits a
    // value of admits fewer levels or refuses preparation.
    for ((name, _), (smaller, default)) in
        SHAPES.iter().zip(admitted(192 * 1024).iter().zip(&default))
    {
        if let Some(default) = default {
            assert!(smaller.is_none_or(|smaller| smaller < *default), "{name}");
        }
    }
    // A larger stack admits every shape, and deeper values of each the default
    // admits.
    for ((name, _), (larger, default)) in SHAPES
        .iter()
        .zip(admitted(1024 * 1024).iter().zip(&default))
    {
        let larger = larger.unwrap_or_else(|| panic!("{name} refused on 1 MiB"));
        assert!(larger >= 1, "{name}");
        if let Some(default) = default {
            assert!(larger > 2 * default, "{name}: {larger} against {default}");
        }
    }
}
