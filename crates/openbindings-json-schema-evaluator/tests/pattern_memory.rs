//! A contract's run matchers and compiled regular expressions share its
//! pattern memory (`Limits::total_pattern_memory`, 64 MiB by default). Each
//! distinct `pattern` form the run matcher does not match and each
//! `patternProperties` name is compiled. A quarter of the memory, up to 2 MiB
//! each, is their search caches, in equal parts. Each run matcher holds the
//! bytes it takes and each form first compiles within 48 KiB, so the memory
//! bounds how many patterns a contract compiles and what its run matchers
//! hold; the forms that need more share what that leaves, each an equal part,
//! and those that need less than their part leave the rest to those that need
//! more. A name that does not fit what it is given refuses preparation, and a
//! `pattern` value, compiled when a string first reaches it, gives that string
//! no verdict.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Map, Value, json};

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

fn prepare(schema: &Value, limits: Limits) -> Result<PreparedValueContract, NoVerdict> {
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::with_limits(limits))
}
fn memory(bytes: usize) -> Limits {
    Limits {
        total_pattern_memory: bytes,
        ..Limits::default()
    }
}
fn value(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn satisfies(contract: &PreparedValueContract, text: &str) -> bool {
    matches!(contract.validate(&value(text)), ValueOutcome::Satisfies)
}
fn no_verdict(contract: &PreparedValueContract, text: &str) -> NoVerdict {
    match contract.validate(&value(text)) {
        ValueOutcome::NoVerdict { detail } => detail,
        _ => panic!("{text} is decided"),
    }
}
// `count` `patternProperties` names of the form `^.{1,10000}N$`, each of which
// compiles to about 13.2 MiB, with `schema` as each name's schema.
fn counted_names(count: usize, schema: Value) -> Value {
    let names: Map<String, Value> = (0..count)
        .map(|i| (format!("^.{{1,10000}}{i}$"), schema.clone()))
        .collect();
    json!({"type": "object", "patternProperties": names})
}
// Properties `p0` and on, each a string holding the matching pattern.
fn pattern_values(patterns: &[String]) -> Value {
    let properties: Map<String, Value> = patterns
        .iter()
        .enumerate()
        .map(|(i, pattern)| (format!("p{i}"), json!({"pattern": pattern})))
        .collect();
    json!({"type": "object", "properties": properties})
}
// `count` small compiled patterns, `a0|b` and on.
fn small(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("a{i}|b")).collect()
}
// The refusal names the original location of the keyword holding the pattern
// past the limit, `/operations/op/input` followed by `keyword`, which may hold
// `*` for one array index.
fn assert_memory_refusal(refused: &NoVerdict, keyword: &str) {
    assert_eq!(refused.code, "pattern-total-memory-limit");
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
    let location = refused.location.as_ref().expect("located refusal");
    let pointer = location.pointer.to_string();
    let expected = format!("/operations/op/input{keyword}");
    let matches = match expected.split_once('*') {
        Some((head, tail)) => {
            pointer.len() > head.len() + tail.len()
                && pointer.starts_with(head)
                && pointer.ends_with(tail)
                && pointer[head.len()..pointer.len() - tail.len()]
                    .bytes()
                    .all(|b| b.is_ascii_digit())
        }
        None => pointer == expected,
    };
    assert!(matches, "{pointer} is not {expected}");
}

#[test]
fn the_default_pattern_memory_is_64_mib() {
    assert_eq!(Limits::default().total_pattern_memory, 64 * MIB);
}

// Forty or three hundred such names once compiled each within the
// per-pattern size, about 28 MB of process memory per name. Together they
// need far more than the memory leaves, and each needs more than an equal
// part, so preparation refuses at their keyword's original location.
#[test]
fn counted_repetitions_that_need_more_together_refuse_preparation() {
    for count in [40, 300] {
        let refused = prepare(&counted_names(count, json!(true)), Limits::default()).unwrap_err();
        assert_memory_refusal(&refused, "/patternProperties");
        let sharing = format!("; the {count} that need more share it equally, but this one");
        assert!(refused.message.contains(&sharing), "{}", refused.message);
    }
}

// A single large counted repetition, alone or among a few patterns, has a
// part large enough to compile, and its contract decides values.
#[test]
fn a_large_counted_repetition_among_few_patterns_compiles() {
    let alone = json!({"type": "object", "patternProperties": {"^.{1,10000}$": false}});
    let few = counted_names(4, json!(false));
    for schema in [&alone, &few] {
        let contract = prepare(schema, Limits::default()).expect("within its part");
        assert!(satisfies(&contract, r#"{"":1}"#));
        assert!(matches!(
            contract.validate(&value(r#"{"a0":1}"#)),
            ValueOutcome::Fails { .. }
        ));
    }
    // Five such names need more than the memory leaves them.
    let refused = prepare(&counted_names(5, json!(false)), Limits::default()).unwrap_err();
    assert_memory_refusal(&refused, "/patternProperties");
}

// A pattern's outcome is local: the patterns that fit their first allowances
// take only that from what a large one may use, so adding them elsewhere
// changes nothing while the contract stays within its pattern memory. Past it,
// the large one refuses.
#[test]
fn small_patterns_elsewhere_leave_a_large_pattern_its_outcome() {
    for count in [0, 10, 100] {
        let schema = json!({
            "type": "object",
            "patternProperties": {"^.{1,10000}$": false},
            "properties": {"small": pattern_values(&small(count))},
        });
        let contract = prepare(&schema, Limits::default()).expect("within the memory");
        assert!(matches!(
            contract.validate(&value(r#"{"a":1}"#)),
            ValueOutcome::Fails { .. }
        ));
        let mut patterns = vec!["^.{1,10000}0$".to_owned()];
        patterns.extend(small(count));
        let contract = prepare(&pattern_values(&patterns), Limits::default()).unwrap();
        assert!(satisfies(&contract, r#"{"p0":"a0"}"#));
    }
    // A thousand more leave less than the large one needs.
    let schema = json!({
        "type": "object",
        "patternProperties": {"^.{1,10000}$": false},
        "properties": {"small": pattern_values(&small(1000))},
    });
    let refused = prepare(&schema, Limits::default()).unwrap_err();
    assert_memory_refusal(&refused, "/patternProperties");
    let mut patterns = vec!["^.{1,10000}0$".to_owned()];
    patterns.extend(small(1000));
    let contract = prepare(&pattern_values(&patterns), Limits::default()).unwrap();
    let refused = no_verdict(&contract, r#"{"p0":"a0"}"#);
    assert_eq!(refused.code, "pattern-total-memory-limit");
    assert!(satisfies(&contract, r#"{"p1":"b"}"#));
}

// A pattern that needs less than its equal part leaves the rest to one that
// needs more, and what each pattern is given does not depend on the order
// strings reach them.
#[test]
fn what_one_pattern_leaves_goes_to_one_that_needs_more() {
    // About 1.3 and 3.9 MiB compiled, within 8 MiB of pattern memory: equal
    // parts of what is left are about 2.9 MiB.
    let patterns = ["^.{1,1000}a$".to_owned(), "^.{1,3000}b$".to_owned()];
    let schema = pattern_values(&patterns);
    for order in [
        [r#"{"p0":"xa"}"#, r#"{"p1":"xb"}"#],
        [r#"{"p1":"xb"}"#, r#"{"p0":"xa"}"#],
    ] {
        let contract = prepare(&schema, memory(8 * MIB)).unwrap();
        for text in order {
            assert!(satisfies(&contract, text), "{text}");
        }
    }
    // A `patternProperties` name shares with the `pattern` values alike: the
    // larger name takes what the smaller value leaves of its part.
    let schema = json!({
        "type": "object",
        "patternProperties": {"^.{1,3000}b$": false},
        "properties": {"p0": {"pattern": "^.{1,1000}a$"}},
    });
    let contract = prepare(&schema, memory(8 * MIB)).expect("the name fits what is left");
    assert!(satisfies(&contract, r#"{"p0":"xa"}"#));
    assert!(matches!(
        contract.validate(&value(r#"{"xb":1}"#)),
        ValueOutcome::Fails { .. }
    ));
    // With a third such pattern, the two larger ones need more together than
    // the smaller leaves them: each refuses, in either order, and the smaller
    // still compiles.
    let patterns = [
        "^.{1,1000}a$".to_owned(),
        "^.{1,3000}b$".to_owned(),
        "^.{1,3000}c$".to_owned(),
    ];
    let schema = pattern_values(&patterns);
    for first in [r#"{"p0":"xa"}"#, r#"{"p2":"xc"}"#] {
        let contract = prepare(&schema, memory(8 * MIB)).unwrap();
        let _ = contract.validate(&value(first));
        assert!(satisfies(&contract, r#"{"p0":"xa"}"#));
        for text in [r#"{"p1":"xb"}"#, r#"{"p2":"xc"}"#] {
            let refused = no_verdict(&contract, text);
            assert_eq!(refused.code, "pattern-total-memory-limit");
            assert!(
                refused
                    .message
                    .contains("those that need less than an equal part of it keep")
                    && refused
                        .message
                        .contains("and the 2 that need more share the rest equally"),
                "{}",
                refused.message
            );
        }
    }
}

// A `pattern` value is compiled when a string first reaches it, within what
// sharing the memory gives it, so a value reaching many large patterns cannot
// spend more than the contract's pattern memory.
#[test]
fn a_pattern_compiled_on_first_use_stays_within_the_memory() {
    let patterns: Vec<String> = (0..40).map(|i| format!("^.{{1,10000}}{i}$")).collect();
    let one = prepare(&pattern_values(&patterns[..1]), Limits::default()).unwrap();
    assert!(satisfies(&one, r#"{"p0":"a0"}"#));
    let many = prepare(&pattern_values(&patterns), Limits::default()).unwrap();
    // A value that reaches no pattern is still decided.
    assert!(satisfies(&many, "{}"));
    let refused = no_verdict(&many, r#"{"p0":"a0"}"#);
    assert_eq!(refused.code, "pattern-total-memory-limit");
    assert_eq!(refused.reason, NoVerdictReason::LimitExceeded);
}

// Each compiled pattern holds 48 KiB, so the pattern memory bounds how many
// patterns a contract compiles: preparation counts them as it reads them and
// refuses at the first past that count, naming its keyword, before compiling
// any. A repeated `pattern` form is compiled once, so it counts once.
#[test]
fn the_compiled_patterns_are_bounded_in_number() {
    let compiled = |count: usize| -> Vec<Value> {
        (0..count)
            .map(|i| json!({"pattern": format!("a{i}|b")}))
            .collect()
    };
    let schema = |items: Vec<Value>| json!({"prefixItems": items});
    // Three patterns' 48 KiB, after a quarter for their search caches.
    let three = memory(3 * 64 * KIB);
    assert!(prepare(&schema(compiled(3)), three.clone()).is_ok());
    let refused = prepare(&schema(compiled(4)), three.clone()).unwrap_err();
    assert_memory_refusal(&refused, "/prefixItems/*/pattern");
    assert_eq!(
        refused.message,
        "this value contract compiles more than 3 regular expressions, as many as its 192 KiB \
         total_pattern_memory holds at 48 KiB each once their search caches take a quarter; \
         raise total_pattern_memory (totalPatternMemory in TypeScript)"
    );
    // Run matchers compile nothing, but they hold their bytes of the same
    // memory: 100 of them, 13,941 bytes, fit beside two compiled patterns,
    // and one holds more than three leave.
    let run_matched: Vec<Value> = (0..100)
        .map(|i| json!({"pattern": format!("^a{{{i}}}$")}))
        .collect();
    assert!(prepare(&schema([compiled(2), run_matched].concat()), three.clone()).is_ok());
    let one = vec![json!({"pattern": "^a{0}$"})];
    let refused = prepare(&schema([compiled(3), one].concat()), three.clone()).unwrap_err();
    assert_memory_refusal(&refused, "/prefixItems/*/pattern");
    assert_eq!(
        refused.message,
        "this value contract's run matchers hold 104 bytes and its 3 compiled regular \
         expressions 48 KiB each, more together than the 144 KiB its 192 KiB \
         total_pattern_memory holds for them once search caches take a quarter; raise \
         total_pattern_memory (totalPatternMemory in TypeScript)"
    );
    let repeated = vec![json!({"pattern": "a|b"}); 100];
    assert!(prepare(&schema(repeated), three.clone()).is_ok());
    let names = json!({"patternProperties": {"a|b": true, "c|d": true, "e|f": true, "g|h": true}});
    assert_memory_refusal(&prepare(&names, three).unwrap_err(), "/patternProperties");
    // The default admits 1,024.
    assert!(prepare(&schema(compiled(1024)), Limits::default()).is_ok());
    let refused = prepare(&schema(compiled(1025)), Limits::default()).unwrap_err();
    assert_memory_refusal(&refused, "/prefixItems/*/pattern");
}

// A small pattern memory refuses a pattern that needs more than it leaves,
// for both keywords, and the refusal states the arithmetic.
#[test]
fn a_small_pattern_memory_refuses_a_pattern_beyond_what_it_leaves() {
    // One pattern in 256 KiB holds 64 KiB for its search cache, which leaves
    // its form 192 KiB; `^.{1,100}x$` compiles to about 135 KiB and
    // `^.{1,1000}x$` to about 1.3 MiB.
    let limits = memory(256 * KIB);
    let small = json!({"patternProperties": {"^.{1,100}x$": false}});
    let contract = prepare(&small, limits.clone()).expect("within what is left");
    assert!(matches!(
        contract.validate(&value(r#"{"ax":1}"#)),
        ValueOutcome::Fails { .. }
    ));
    let large = json!({"patternProperties": {"^.{1,1000}x$": false}});
    assert_memory_refusal(
        &prepare(&large, limits.clone()).unwrap_err(),
        "/patternProperties",
    );
    let contract = prepare(&json!({"pattern": "^.{1,1000}x$"}), limits).unwrap();
    let refused = no_verdict(&contract, r#""ax""#);
    assert_eq!(refused.code, "pattern-total-memory-limit");
    assert_eq!(
        refused.message,
        "this value contract's one compiled regular expression holds 48 KiB of its 256 KiB \
         total_pattern_memory and 64 KiB for its search cache, which leaves 144 KiB; this \
         one, which needs more, may take it all, but does not compile within the 192 KiB \
         that gives it; raise total_pattern_memory (totalPatternMemory in TypeScript)"
    );
    // Two such names in 2 MiB: each needs more than an equal part.
    let two = json!({"patternProperties": {"^.{1,1000}x$": false, "^.{1,1000}y$": false}});
    let refused = prepare(&two, memory(2 * MIB)).unwrap_err();
    assert_memory_refusal(&refused, "/patternProperties");
    assert_eq!(
        refused.message,
        "this value contract's 2 compiled regular expressions hold 48 KiB each of its 2 MiB \
         total_pattern_memory and 256 KiB each for their search caches, which leaves 1.4 MiB; \
         the 2 that need more share it equally, but this one does not compile within the \
         768 KiB that gives it; raise total_pattern_memory (totalPatternMemory in TypeScript)"
    );
    // No pattern memory admits no compiled pattern and no run matcher, and
    // leaves patterns the evaluator does not match and contracts without
    // patterns alone.
    let none = memory(0);
    assert_memory_refusal(
        &prepare(&json!({"pattern": "a|b"}), none.clone()).unwrap_err(),
        "/pattern",
    );
    let refused = prepare(&json!({"pattern": "^[a-z]+$"}), none.clone()).unwrap_err();
    assert_memory_refusal(&refused, "/pattern");
    assert_eq!(
        refused.message,
        "this value contract's run matchers hold 128 bytes, more than the 0 bytes its \
         0 bytes total_pattern_memory holds for them once search caches take a quarter; \
         raise total_pattern_memory (totalPatternMemory in TypeScript)"
    );
    let unmatched = prepare(&json!({"pattern": "^(?=a)"}), none.clone()).unwrap();
    assert_eq!(
        no_verdict(&unmatched, r#""a""#).code,
        "pattern-backtracking"
    );
    assert!(prepare(&json!({"type": "string"}), none).is_ok());
}

// A run matcher holds a step for each change of set and a byte for each byte
// of literal text, so a long literal pattern holds about its own length, and
// the pattern memory counts it. 380 distinct literal patterns of 10,002 bytes,
// 3.8 MB within the default total_pattern_bytes, each hold 10,116 bytes:
// 3.7 MiB, within the default memory, where they once held about 1 MB each.
#[test]
fn run_matchers_hold_about_their_text_of_the_pattern_memory() {
    let literal = |i: usize| format!("^t{i:03}x{}$", "a".repeat(9_995));
    let patterns: Vec<String> = (0..380).map(literal).collect();
    assert!(patterns.iter().all(|pattern| pattern.len() == 10_002));
    let contract = prepare(&pattern_values(&patterns), Limits::default()).unwrap();
    let text = &patterns[7][1..10_001];
    assert!(satisfies(&contract, &json!({ "p7": text }).to_string()));
    assert!(matches!(
        contract.validate(&value(&json!({ "p7": &text[1..] }).to_string())),
        ValueOutcome::Fails { .. }
    ));
    // Three quarters of 4 MiB hold 310 of them and not 311.
    let limits = memory(4 * MIB);
    assert!(prepare(&pattern_values(&patterns[..310]), limits.clone()).is_ok());
    let refused = prepare(&pattern_values(&patterns[..311]), limits).unwrap_err();
    assert_memory_refusal(&refused, "/properties/p*/pattern");
    assert_eq!(
        refused.message,
        "this value contract's run matchers hold 3.1 MiB, more than the 3 MiB its 4 MiB \
         total_pattern_memory holds for them once search caches take a quarter; raise \
         total_pattern_memory (totalPatternMemory in TypeScript)"
    );
}
