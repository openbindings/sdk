//! The unevaluated walk limit keeps preparation within the stack the host gives
//! (`Limits::stack_bytes`). The library compiles an `unevaluatedProperties`
//! keyword's annotation walk by recursion, and the `$ref` targets it is
//! compiling when it reaches the holder (up to eight before it defers the next)
//! add to that stack. Each kind of walk, as long as the limit for a stack size
//! admits, prepares on a thread of that size below every depth of enclosing
//! `properties` up to 40, where those targets peak, with a fifth of the thread
//! already used, so every host keeps that much headroom; one level more
//! refuses. A larger stack admits longer walks and a smaller one refuses them
//! sooner. The only test in this executable, so nothing else shares its thread.
//!
//! Release builds with Rust 1.99.0 on GitHub's hosted runners, 2026-10-21: the
//! most stack that can already be in use on a 256 KiB thread when the longest
//! admitted walk of each kind still prepares at its worst enclosing depth, and
//! that share of the thread; each row gives its kind with the least to spare.
//! The thread itself leaves 249.5 KiB usable on Linux, 225.5 KiB on Windows and
//! 255.5 KiB on macOS. `docs/performance-boundaries.md` gives the same
//! measurement from 128 KiB to 2 MiB; Windows keeps at least 21% from 192 KiB.
//!
//! | Walk | Levels | Linux x86_64 | Windows x86_64 | macOS arm64 |
//! | --- | --- | --- | --- | --- |
//! | `$ref` | 26 | 91.5 KiB, 36% | 57.0 KiB, 22% | 108.0 KiB, 42% |
//! | `allOf`, `anyOf` or `oneOf` | 8 | 91.5 KiB, 36% | 57.0 KiB, 22% | 107.5 KiB, 42% |
//! | `then` or `else` | 7 | 97.0 KiB, 38% | 61.5 KiB, 24% | 113.0 KiB, 44% |
//! | `dependentSchemas` | 7 | 93.0 KiB, 36% | 58.5 KiB, 23% | 109.0 KiB, 43% |
//! | A holder at every level | 21, 7, 6 | 103.0 KiB, 40% | 69.0 KiB, 27% | 119.5 KiB, 47% |
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
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
fn prepare(schema: &str, evaluator: &DefaultEvaluator) -> Result<PreparedValueContract, NoVerdict> {
    let resources = ResourceSet::new([SchemaResource {
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
    .prepare_with(evaluator, &resources)
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
        // The default evaluator assumes the default stack.
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
                    assert_eq!(
                        longer.err().map(|detail| detail.code),
                        Some("unevaluated-walk-limit".into()),
                        "{stack}: {kind} nested={nested} {} levels",
                        levels + 1
                    );
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
