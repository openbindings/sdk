//! Preserved deep annotation-walk workloads on explicitly sized native threads.
//! These are measurements of this build, not a portable stack-size guarantee.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};

// One level of a walk of `kind` to the definition `next`.
fn hop(kind: &str, next: Value) -> Value {
    match kind {
        "$ref" => next,
        "allOf" => json!({ "allOf": [next] }),
        "anyOf" => json!({ "anyOf": [next] }),
        "oneOf" => json!({ "oneOf": [next] }),
        "then" => json!({ "if": true, "then": next }),
        "else" => json!({ "if": false, "else": next }),
        "dependentSchemas" => json!({ "dependentSchemas": { "a": next } }),
        other => unreachable!("{other}"),
    }
}
// A holder `nesting` levels of `properties` deep whose walk runs `levels`
// levels of `kind`, alternating `dependentSchemas` and `then` for "mixed", with
// a holder at every level when `nested`.
fn schema(kind: &str, nested: bool, levels: usize, nesting: usize) -> String {
    let kind_at = |level: usize| match kind {
        "mixed" if level.is_multiple_of(2) => "dependentSchemas",
        "mixed" => "then",
        kind => kind,
    };
    let reference = |level: usize| json!({ "$ref": format!("#/$defs/c{level}") });
    let mut definitions = serde_json::Map::new();
    for level in 1..levels {
        let mut definition = hop(kind_at(level), reference(level + 1));
        if nested {
            definition["unevaluatedProperties"] = json!(false);
        }
        definitions.insert(format!("c{level}"), definition);
    }
    definitions.insert(format!("c{levels}"), json!({ "properties": { "z": true } }));
    let mut holder = hop(kind_at(0), reference(1));
    holder["unevaluatedProperties"] = json!(false);
    for _ in 0..nesting {
        holder = json!({ "properties": { "p": holder } });
    }
    holder["$defs"] = Value::Object(definitions);
    holder.to_string()
}
fn prepare(
    schema: &str,
    evaluator: &DefaultEvaluator,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://stack.test/s".into(),
        document: JsonValue::parse(schema).unwrap(),
    }])
    .unwrap();
    Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://stack.test/s"}}}}"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
    .resolve_operation("op")
    .unwrap()
    .input()
    .unwrap()
    .prepare(
        evaluator,
        &resources,
        &openbindings::PreparationLimits::default(),
    )
}

// Run `then` once at least `bytes` of the stack below `top` are in use.
#[inline(never)]
fn below(top: usize, bytes: usize, then: &mut dyn FnMut()) {
    let frame = std::hint::black_box([0u8; 512]);
    if top.saturating_sub(&frame as *const _ as usize) >= bytes {
        then();
    } else {
        below(top, bytes, then);
    }
    std::hint::black_box(&frame);
}

// Each kind of walk, with a holder at every level or not.
const KINDS: [(&str, bool); 13] = [
    ("$ref", false),
    ("$ref", true),
    ("allOf", false),
    ("allOf", true),
    ("anyOf", false),
    ("oneOf", false),
    ("then", false),
    ("then", true),
    ("else", false),
    ("dependentSchemas", false),
    ("dependentSchemas", true),
    ("mixed", false),
    ("mixed", true),
];
// Each stack size and the most levels of each kind its walk limit admits: a
// reference weighs 4, each `allOf`, `anyOf` or `oneOf` on its path 5 more, each
// `then`, `else` or `dependentSchemas` 7 more and each holder reached 1 more,
// against 105 at the default 256 KiB, 54 at 192 KiB and 310 at 512 KiB.
const STACKS: [(usize, [usize; 13]); 3] = [
    (192 * 1024, [13, 11, 4, 3, 4, 4, 3, 3, 3, 3, 3, 3, 3]),
    (256 * 1024, [26, 21, 8, 7, 8, 8, 7, 6, 7, 7, 6, 7, 6]),
    (
        512 * 1024,
        [77, 62, 23, 22, 23, 23, 20, 19, 20, 20, 19, 20, 19],
    ),
];

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "unoptimized compilation frames exceed 256 KiB at the walk limit; verified in release"
)]
fn the_longest_admitted_walks_prepare_on_a_stack_of_the_size_given() {
    for (stack, levels) in STACKS {
        let evaluator = DefaultEvaluator::new();
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(move || {
                let marker = 0u8;
                let top = std::hint::black_box(&marker) as *const u8 as usize;
                for ((kind, nested), levels) in KINDS.into_iter().zip(levels) {
                    for nesting in (0..=40).step_by(2) {
                        let text = schema(kind, nested, levels, nesting);
                        below(top, stack / 5, &mut || {
                            let admitted = prepare(&text, &evaluator);
                            assert!(
                                admitted.is_ok(),
                                "{stack}: {kind} nested={nested} {levels} levels under {nesting}: {:?}",
                                admitted.err()
                            );
                        });
                    }
                    let longer = prepare(&schema(kind, nested, levels + 1, 0), &evaluator);
                    assert!(longer.is_ok(), "{stack}: {kind} nested={nested} {} levels: {:?}", levels + 1, longer.err());
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
