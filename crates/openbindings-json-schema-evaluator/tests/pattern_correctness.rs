use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};

fn prepare(
    schema: Value,
    resources: ExternalSchemas,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    prepare_with_limits(schema, resources, &PreparationLimits::default())
}
fn prepare_with_limits(
    schema: Value,
    resources: ExternalSchemas,
    limits: &PreparationLimits,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
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
        .prepare(&DefaultEvaluator::new(), &resources, limits)
}

// Patterns with lone surrogate escapes, matched as Node's `RegExp` with the `u`
// flag matches them, through both pattern keywords. A lookbehind needs a
// backtracking engine: a string that reaches one gives no verdict, and so
// does an object whose verdict depends on a `patternProperties` name holding
// one.
#[test]
fn node_surrogate_differential_for_both_pattern_keywords() {
    let mut refused = 0;
    for row in include_str!("fixtures/lone-surrogate.tsv").lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        let [id, pattern, input, expected] = fields.as_slice() else {
            panic!("{row}")
        };
        let backtracking = pattern.starts_with("(?<=");
        for (keyword, schema, value) in [
            ("pattern", json!({"pattern":pattern}), json!(input)),
            (
                "patternProperties",
                json!({"patternProperties":{*pattern:true},"additionalProperties":false}),
                json!({*input:null}),
            ),
        ] {
            let contract = match prepare(schema, ExternalSchemas::default()) {
                Ok(contract) => contract,
                Err(detail) => panic!("{id}/{keyword}: {detail:?}"),
            };
            let value = JsonValue::parse(value.to_string()).unwrap();
            match contract.evaluate(&value) {
                ValueOutcome::Satisfies => assert_eq!(*expected, "true", "{id}/{keyword}"),
                ValueOutcome::Fails { .. } => assert_eq!(*expected, "false", "{id}/{keyword}"),
                ValueOutcome::NoVerdict { detail } if backtracking => {
                    assert_eq!(detail.code, "pattern-backtracking", "{id}/{keyword}");
                    refused += 1;
                }
                other => panic!("{id}/{keyword}: {other:?}"),
            }
        }
    }
    // Four lookbehind patterns, five strings each, unmatched through each
    // keyword.
    assert_eq!(refused, 40);
}

#[test]
fn invalid_patterns_locate_authored_keywords_in_document_and_supplied_resources() {
    const URI: &str = "https://patterns.invalid/supplied";
    // Each value's verdict depends on the invalid pattern's match.
    for (keyword, invalid, reaching) in [
        ("pattern", json!({"pattern":"[SECRET/~%"}), json!("x")),
        (
            "patternProperties",
            json!({"patternProperties":{"[SECRET/~%":false}}),
            json!({"k":1}),
        ),
    ] {
        for supplied in [false, true] {
            let schema = json!({"properties":{"a/b~c":invalid}});
            let (entry, resources, resource, prefix) = if supplied {
                (
                    json!({"$ref":URI}),
                    ExternalSchemas::new([ExternalSchema {
                        uri: URI.into(),
                        document: JsonValue::parse(schema.to_string()).unwrap(),
                    }])
                    .unwrap(),
                    Some(URI.to_owned()),
                    "",
                )
            } else {
                (
                    schema,
                    ExternalSchemas::default(),
                    None,
                    "/operations/op/input",
                )
            };
            let contract = prepare(entry, resources).unwrap_or_else(|d| panic!("{d:?}"));
            let value = JsonValue::parse(json!({ "a/b~c": reaching }).to_string()).unwrap();
            let ValueOutcome::Undefined { detail } = contract.evaluate(&value) else {
                panic!("{keyword}/{supplied}")
            };
            assert_eq!(detail.code, "semantic-dependence");
            assert_eq!(
                detail.location,
                Some(SchemaOrigin::new(
                    resource,
                    format!("{prefix}/properties/a~1b~0c/{keyword}")
                        .parse()
                        .unwrap(),
                ))
            );
            assert!(!detail.message.contains("SECRET"));
        }
    }
}

#[derive(serde::Deserialize)]
struct Ecma {
    rows: Vec<Units>,
}
// A row as the fixture spells it, in UTF-16 code units.
#[derive(serde::Deserialize)]
struct Units {
    pattern: Vec<u16>,
    strings: Vec<Vec<u16>>,
    matches: Option<String>,
}
struct EcmaRow {
    pattern: String,
    strings: Vec<String>,
    // V8's result for each string, `1` or `0`, or `-` where V8 departs from
    // ECMA-262; `None` when V8 refuses the pattern.
    matches: Option<String>,
}
fn ecma_rows() -> Vec<EcmaRow> {
    let fixture: Ecma = serde_json::from_str(include_str!("fixtures/pattern-ecma.json")).unwrap();
    let text = |units: &[u16]| String::from_utf16(units).expect("scalar fixture text");
    fixture
        .rows
        .into_iter()
        .map(|row| EcmaRow {
            pattern: text(&row.pattern),
            strings: row.strings.iter().map(|s| text(s)).collect(),
            matches: row.matches,
        })
        .collect()
}
// Whether a pattern holds a construct the evaluator does not match:
// lookaround, a backreference, a property absent from the pinned tables or a
// non-word boundary. Some of these patterns have a supported linear form.
fn may_be_unmatched(pattern: &str) -> bool {
    ["(?=", "(?!", "(?<=", "(?<!", "\\k", "\\p", "\\P", "\\B"]
        .iter()
        .any(|construct| pattern.contains(construct))
        || pattern
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'\\' && (b'1'..=b'9').contains(&pair[1]))
}
const UNMATCHED: [&str; 3] = [
    "pattern-backtracking",
    "unicode-property-matching",
    "pattern-non-word-boundary",
];
// The property each row's pattern applies to in a combined contract.
fn member(index: usize) -> String {
    format!("p{index}")
}
// Prepare one contract holding each row's schema under its member.
fn combined(
    rows: &[(usize, &EcmaRow)],
    schema: impl Fn(&str) -> Value,
) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
    let properties: serde_json::Map<String, Value> = rows
        .iter()
        .map(|(index, row)| (member(*index), schema(&row.pattern)))
        .collect();
    // This batches the entire independently generated matching corpus into one
    // document for test speed. Its cumulative Unicode translation work exceeds
    // an ordinary request's default allowance; keep a finite explicit budget.
    prepare_with_limits(
        json!({ "properties": properties }),
        ExternalSchemas::default(),
        &PreparationLimits {
            max_work_units: 256 * 1024 * 1024,
            max_allocation_bytes: 1024 * 1024 * 1024,
            ..PreparationLimits::default()
        },
    )
    .unwrap_or_else(|detail| panic!("{:?}", detail.kind))
}

// Generated patterns, matched as V8's `RegExp` with the `u` flag matches them,
// through both pattern keywords and the names an `unevaluatedProperties`
// keyword reads as evaluated. The evaluator reads ECMA-262 11th edition, so a
// pattern V8 accepts may be read as invalid only for a property escape whose
// name or value that edition lacks; it declines to match only the constructs
// that need a backtracking engine, missing property tables and `\B`, and a string
// whose verdict depends on an invalid pattern gives no verdict.
#[test]
fn generated_patterns_match_as_ecmascript_does_through_both_keywords() {
    let rows = ecma_rows();
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut valid = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row.matches.is_some() {
            valid.push((index, row));
            continue;
        }
        for (schema, value) in [
            (json!({"pattern": row.pattern}), json!("x")),
            (
                json!({"patternProperties": {row.pattern.as_str(): false}}),
                json!({"x": 0}),
            ),
        ] {
            let contract = prepare(schema, ExternalSchemas::default())
                .unwrap_or_else(|detail| panic!("{:?}: {detail:?}", row.pattern));
            let value = JsonValue::parse(value.to_string()).unwrap();
            let ValueOutcome::Undefined { detail } = contract.evaluate(&value) else {
                panic!("V8 refuses {:?}", row.pattern)
            };
            assert_eq!(detail.code, "semantic-dependence", "{:?}", row.pattern);
            *counts.entry("invalid stands in").or_default() += 1;
        }
    }
    // V8 is not an oracle only for `\B`, which the evaluator never matches,
    // so no verdict is ever compared with such a result.
    for row in rows
        .iter()
        .filter(|row| row.matches.as_ref().is_some_and(|m| m.contains('-')))
    {
        assert!(row.pattern.contains("\\B"), "{:?}", row.pattern);
    }
    let expected = |row: &EcmaRow, at: usize| match row.matches.as_ref().unwrap().as_bytes()[at] {
        b'1' => true,
        b'0' => false,
        _ => unreachable!("{:?} has no oracle at {at}", row.pattern),
    };
    // `pattern`: a pattern the evaluator does not match gives no verdict for a
    // string that reaches it.
    let contract = combined(&valid, |p| json!({"pattern": p}));
    let mut excluded = std::collections::BTreeSet::new();
    for &(index, row) in &valid {
        for (at, string) in row.strings.iter().enumerate() {
            let value = JsonValue::parse(json!({ member(index): string }).to_string()).unwrap();
            match contract.evaluate(&value) {
                ValueOutcome::Satisfies if expected(row, at) => {}
                ValueOutcome::Fails { .. } if !expected(row, at) => {}
                ValueOutcome::NoVerdict { detail }
                    if UNMATCHED.contains(&detail.code.as_ref())
                        && may_be_unmatched(&row.pattern) =>
                {
                    *counts.entry("pattern unmatched").or_default() += 1;
                    continue;
                }
                ValueOutcome::NoVerdict { detail }
                    if detail.code == "schema-pattern-compilation"
                        && (row.pattern.contains("\\p") || row.pattern.contains("\\P")) =>
                {
                    if excluded.insert(index) {
                        *counts
                            .entry("property escape outside the edition")
                            .or_default() += 1;
                    }
                    continue;
                }
                other => panic!("pattern {:?} on {string:?}: {other:?}", row.pattern),
            }
            *counts.entry("pattern agrees").or_default() += 1;
        }
    }
    // `patternProperties`: an object whose verdict depends on a name the
    // evaluator does not match gives no verdict, so those rows are prepared
    // alone.
    let (alone, together): (Vec<_>, Vec<_>) = valid
        .iter()
        .filter(|(index, _)| !excluded.contains(index))
        .copied()
        .partition(|(_, row)| may_be_unmatched(&row.pattern));
    for (_, row) in alone {
        let contract = prepare(
            json!({"patternProperties": {row.pattern.as_str(): false}}),
            ExternalSchemas::default(),
        )
        .unwrap_or_else(|detail| panic!("{:?}: {detail:?}", row.pattern));
        let mut unmatched = 0;
        for (at, string) in row.strings.iter().enumerate() {
            let value = JsonValue::parse(json!({ string: 0 }).to_string()).unwrap();
            match contract.evaluate(&value) {
                ValueOutcome::NoVerdict { detail } if UNMATCHED.contains(&detail.code.as_ref()) => {
                    unmatched += 1;
                }
                outcome => {
                    let fails = matches!(outcome, ValueOutcome::Fails { .. });
                    assert_eq!(fails, expected(row, at), "{:?} on {string:?}", row.pattern);
                    *counts.entry("patternProperties agrees").or_default() += 1;
                }
            }
        }
        // A name the evaluator does not match leaves every string undecided.
        assert!(
            unmatched == 0 || unmatched == row.strings.len(),
            "{:?}",
            row.pattern
        );
        if unmatched > 0 {
            *counts.entry("patternProperties unmatched").or_default() += 1;
        } else {
            let marked = prepare(
                json!({"patternProperties": {row.pattern.as_str(): true}, "unevaluatedProperties": false}),
                ExternalSchemas::default(),
            ).unwrap();
            for (at, string) in row.strings.iter().enumerate() {
                let value = JsonValue::parse(json!({ string: 0 }).to_string()).unwrap();
                let outcome = marked.evaluate(&value);
                assert!(matches!(
                    outcome,
                    ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                ));
                assert_eq!(
                    matches!(outcome, ValueOutcome::Satisfies),
                    expected(row, at)
                );
                *counts.entry("unevaluated marking agrees").or_default() += 1;
            }
        }
    }
    let contract = combined(&together, |p| json!({"patternProperties": {p: false}}));
    for &(index, row) in &together {
        for (at, string) in row.strings.iter().enumerate() {
            let value =
                JsonValue::parse(json!({ member(index): { string: 0 } }).to_string()).unwrap();
            let fails = matches!(contract.evaluate(&value), ValueOutcome::Fails { .. });
            assert_eq!(fails, expected(row, at), "{:?} on {string:?}", row.pattern);
            *counts.entry("patternProperties agrees").or_default() += 1;
        }
    }
    // An `unevaluatedProperties` keyword reads the same names as evaluated
    // when they match.
    let contract = combined(
        &together,
        |p| json!({"patternProperties": {p: true}, "unevaluatedProperties": false}),
    );
    for &(index, row) in &together {
        for (at, string) in row.strings.iter().enumerate() {
            let value =
                JsonValue::parse(json!({ member(index): { string: 0 } }).to_string()).unwrap();
            let satisfies = matches!(contract.evaluate(&value), ValueOutcome::Satisfies);
            assert_eq!(
                satisfies,
                expected(row, at),
                "{:?} on {string:?}",
                row.pattern
            );
            *counts.entry("unevaluated marking agrees").or_default() += 1;
        }
    }
    // 1,247 generated patterns with eight strings each: 305 V8 refuses,
    // through both keywords; 88 hold constructs the evaluator does not match.
    // Then 130 fixed patterns with sets that meet across the surrogate gap,
    // with 13 strings each, and 128 with word-boundary assertions, with 15
    // strings each; the 64 holding `\B` are not matched.
    assert_eq!(
        counts,
        [
            ("invalid stands in", 610),
            ("pattern agrees", 9_482),
            ("pattern unmatched", 1_664),
            ("patternProperties agrees", 9_482),
            ("patternProperties unmatched", 152),
            ("unevaluated marking agrees", 9_482),
        ]
        .into()
    );
}

// The regex crate's `is_match` answers no for `\x{E9}|(?-u:\B)` on "aéa",
// whose first branch matches, because the non-word boundary can hold inside a
// multi-byte character. So `\B` is not matched: a string that reaches it gives
// no verdict, as does an object whose verdict depends on a `patternProperties`
// name holding it, and an `unevaluatedProperties` keyword reading such a name
// refuses preparation. `\b` keeps its verdicts.
#[test]
fn a_non_word_boundary_is_not_matched_and_a_word_boundary_is() {
    for (pattern, value) in [
        ("^.*$|\\B", "aéa"),
        ("[^a]|\\B", "a中a"),
        ("\\W|\\B", "a😀a"),
        ("\\B", "abc"),
    ] {
        let contract = prepare(json!({"pattern": pattern}), ExternalSchemas::default()).unwrap();
        let text = value;
        let value = JsonValue::parse(json!(text).to_string()).unwrap();
        let ValueOutcome::NoVerdict { detail } = contract.evaluate(&value) else {
            panic!("{pattern}")
        };
        assert_eq!(detail.code, "pattern-non-word-boundary", "{pattern}");
        assert_eq!(detail.reason, NoVerdictReason::UnsupportedCapability);
        let contract = prepare(
            json!({"patternProperties": {pattern: {"type": "integer"}}}),
            ExternalSchemas::default(),
        )
        .unwrap();
        let object = JsonValue::parse(json!({ text: "s" }).to_string()).unwrap();
        let ValueOutcome::NoVerdict { detail } = contract.evaluate(&object) else {
            panic!("{pattern}")
        };
        assert_eq!(detail.code, "pattern-non-word-boundary", "{pattern}");
        assert!(matches!(
            contract.evaluate(&JsonValue::parse(json!({ text: 1 }).to_string()).unwrap()),
            ValueOutcome::Satisfies
        ));
        let contract = prepare(
            json!({"patternProperties": {pattern: true}, "unevaluatedProperties": false}),
            ExternalSchemas::default(),
        )
        .unwrap();
        let ValueOutcome::NoVerdict { detail } = contract.evaluate(&object) else {
            panic!("{pattern}");
        };
        assert_eq!(detail.reason, NoVerdictReason::UnsupportedCapability);
        assert_eq!(detail.code, "pattern-non-word-boundary");
    }
    for (pattern, value, satisfies) in [
        ("^a\\b", "a", true),
        ("^a\\b", "ab", false),
        ("^a\\b", "aé", true),
        ("^.*$|\\b", "aéa", true),
        ("é\\b", "aéa", true),
        ("\\bé", "aéa", true),
        ("é\\b", "éé", false),
        ("a\\b", "a😀", true),
    ] {
        let contract = prepare(json!({"pattern": pattern}), ExternalSchemas::default()).unwrap();
        let outcome = contract.evaluate(&JsonValue::parse(json!(value).to_string()).unwrap());
        assert_eq!(
            matches!(outcome, ValueOutcome::Satisfies),
            satisfies,
            "{pattern} on {value}: {outcome:?}"
        );
    }
}

// Each pattern the evaluator does not match keeps its own reason, whichever a
// contract holds first.
#[test]
fn unmatched_patterns_keep_their_own_reasons() {
    let wide = "\\S".repeat(6000);
    for (first, second) in [("(?!a)", wide.as_str()), (wide.as_str(), "(?!a)")] {
        let contract = prepare(
            json!({"properties": {"a": {"pattern": first}, "b": {"pattern": second}, "c": {"pattern": "\\Bx"}}}),
            ExternalSchemas::default(),
        )
        .unwrap();
        for (member, pattern) in [("a", first), ("b", second), ("c", "\\Bx")] {
            let value = JsonValue::parse(json!({ member: "x" }).to_string()).unwrap();
            let ValueOutcome::NoVerdict { detail } = contract.evaluate(&value) else {
                panic!("{member}")
            };
            let expected = if pattern.starts_with("(?") {
                "pattern-backtracking"
            } else if pattern.starts_with("\\B") {
                "pattern-non-word-boundary"
            } else {
                "pattern-size-limit"
            };
            assert_eq!(detail.code, expected, "{member}");
        }
    }
}

// Independently generated JS-u observations, with edition11 invalid-name
// controls kept separate from current Node's wider accepted property table.
#[test]
fn independently_reviewed_property_and_assertion_capabilities() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/pattern-capabilities.json")).unwrap();
    for row in fixture["rows"].as_array().unwrap() {
        let pattern = row["pattern"].as_str().unwrap();
        let capability = row["expected_capability"].as_str().unwrap();
        for schema in [
            json!({"pattern":pattern}),
            json!({"patternProperties":{pattern:true},"unevaluatedProperties":false}),
        ] {
            let is_name = schema.get("patternProperties").is_some();
            // Unsupported patterns cannot supply the evaluated-name annotation.
            if is_name && capability != "linear" {
                continue;
            }
            let prepared = prepare(schema, ExternalSchemas::default()).unwrap();
            for (at, string) in fixture["strings"].as_array().unwrap().iter().enumerate() {
                let value = if is_name {
                    json!({string.as_str().unwrap():null})
                } else {
                    string.clone()
                };
                let outcome = prepared.evaluate(&JsonValue::parse(value.to_string()).unwrap());
                match capability {
                    "linear" => {
                        let expected =
                            row["semantic_matches"].as_str().unwrap().as_bytes()[at] == b'1';
                        assert!(
                            matches!(
                                outcome,
                                ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                            ),
                            "{}: {outcome:?}",
                            row["id"]
                        );
                        assert_eq!(
                            matches!(outcome, ValueOutcome::Satisfies),
                            expected,
                            "{} at {at}",
                            row["id"]
                        );
                    }
                    "invalid" => assert!(
                        matches!(outcome, ValueOutcome::Undefined { .. }),
                        "{}: {outcome:?}",
                        row["id"]
                    ),
                    "unsupported" => assert!(
                        matches!(outcome, ValueOutcome::NoVerdict { ref detail } if detail.reason == NoVerdictReason::UnsupportedCapability),
                        "{}: {outcome:?}",
                        row["id"]
                    ),
                    _ => panic!("unknown fixture capability"),
                }
            }
        }
    }
}
