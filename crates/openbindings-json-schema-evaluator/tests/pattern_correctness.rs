use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};

fn prepare(schema: Value, resources: ResourceSet) -> Result<PreparedValueContract, NoVerdict> {
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
        .prepare_with(&DefaultEvaluator::new(), &resources)
}

// Patterns with lone surrogate escapes, matched as Node's `RegExp` with the `u`
// flag matches them, through both pattern keywords. A lookbehind needs a
// backtracking engine: a string that reaches one gives no verdict, and a
// `patternProperties` name holding one refuses preparation.
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
            let contract = match prepare(schema, ResourceSet::default()) {
                Ok(contract) => contract,
                Err(detail) if backtracking && keyword == "patternProperties" => {
                    assert_eq!(detail.code, "pattern-backtracking", "{id}/{keyword}");
                    refused += 1;
                    continue;
                }
                Err(detail) => panic!("{id}/{keyword}: {detail:?}"),
            };
            let value = JsonValue::parse(value.to_string()).unwrap();
            match contract.validate(&value) {
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
    // Four lookbehind patterns, five strings each, refused through each keyword.
    assert_eq!(refused, 40);
}

#[test]
fn compilation_refusals_locate_authored_keywords_in_document_and_supplied_resources() {
    const URI: &str = "https://patterns.invalid/supplied";
    for (keyword, invalid) in [
        ("pattern", json!({"pattern":"[SECRET/~%"})),
        (
            "patternProperties",
            json!({"patternProperties":{"[SECRET/~%":true}}),
        ),
    ] {
        for supplied in [false, true] {
            let schema = json!({"properties":{"a/b~c":invalid}});
            let (entry, resources, resource, prefix) = if supplied {
                (
                    json!({"$ref":URI}),
                    ResourceSet::new([SchemaResource {
                        uri: URI.into(),
                        document: JsonValue::parse(schema.to_string()).unwrap(),
                    }])
                    .unwrap(),
                    Some(URI.to_owned()),
                    "",
                )
            } else {
                (schema, ResourceSet::default(), None, "/operations/op/input")
            };
            let Err(detail) = prepare(entry, resources) else {
                panic!("{keyword}/{supplied}")
            };
            assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
            assert_eq!(detail.code, "schema-pattern-compilation");
            assert_eq!(
                detail.location,
                Some(SchemaLocation::new(
                    resource,
                    format!("{prefix}/properties/a~1b~0c/{keyword}")
                        .parse()
                        .unwrap(),
                ))
            );
            assert_eq!(
                detail.message,
                format!("the schema {keyword} regular expression could not be compiled")
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
// lookaround, a backreference, a property escape or a non-word boundary.
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
// Prepare one contract holding each row's schema under its member, leaving out
// each row whose pattern preparation refuses as invalid; `refused` checks that
// refusal. Returns the contract and the rows left out.
fn combined(
    rows: &[(usize, &EcmaRow)],
    schema: impl Fn(&str) -> Value,
    keyword: &str,
    mut refused: impl FnMut(usize),
) -> (PreparedValueContract, std::collections::BTreeSet<usize>) {
    let mut excluded = std::collections::BTreeSet::new();
    loop {
        let properties: serde_json::Map<String, Value> = rows
            .iter()
            .filter(|(index, _)| !excluded.contains(index))
            .map(|(index, row)| (member(*index), schema(&row.pattern)))
            .collect();
        match prepare(json!({ "properties": properties }), ResourceSet::default()) {
            Ok(contract) => return (contract, excluded),
            Err(detail) => {
                assert_eq!(detail.code, "schema-pattern-compilation", "{detail:?}");
                let pointer = detail
                    .location
                    .expect("located refusal")
                    .pointer
                    .to_string();
                let index: usize = pointer
                    .strip_prefix("/operations/op/input/properties/p")
                    .and_then(|rest| rest.strip_suffix(&format!("/{keyword}")))
                    .and_then(|index| index.parse().ok())
                    .unwrap_or_else(|| panic!("{pointer}"));
                assert!(excluded.insert(index), "{pointer}");
                refused(index);
            }
        }
    }
}

// Generated patterns, matched as V8's `RegExp` with the `u` flag matches them,
// through both pattern keywords and the names an `unevaluatedProperties`
// keyword reads as evaluated. The evaluator reads ECMA-262 11th edition, so a
// pattern V8 accepts may be refused as invalid only for a property escape
// whose name or value that edition lacks; it declines to match only the
// constructs that need a backtracking engine, property escapes and `\B`.
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
        for schema in [
            json!({"pattern": row.pattern}),
            json!({"patternProperties": {row.pattern.as_str(): false}}),
        ] {
            let Err(detail) = prepare(schema, ResourceSet::default()) else {
                panic!("V8 refuses {:?}", row.pattern)
            };
            assert_eq!(
                detail.code, "schema-pattern-compilation",
                "{:?}",
                row.pattern
            );
            *counts.entry("invalid refused").or_default() += 1;
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
    let mut invalid_property = |index: usize| {
        let pattern = &rows[index].pattern;
        assert!(
            pattern.contains("\\p") || pattern.contains("\\P"),
            "{pattern:?}"
        );
        *counts
            .entry("property escape outside the edition")
            .or_default() += 1;
    };
    // `pattern`: a pattern the evaluator does not match gives no verdict for a
    // string that reaches it.
    let (contract, excluded) = combined(
        &valid,
        |p| json!({"pattern": p}),
        "pattern",
        &mut invalid_property,
    );
    for &(index, row) in valid.iter().filter(|(index, _)| !excluded.contains(index)) {
        for (at, string) in row.strings.iter().enumerate() {
            let value = JsonValue::parse(json!({ member(index): string }).to_string()).unwrap();
            match contract.validate(&value) {
                ValueOutcome::Satisfies if expected(row, at) => {}
                ValueOutcome::Fails { .. } if !expected(row, at) => {}
                ValueOutcome::NoVerdict { detail }
                    if UNMATCHED.contains(&detail.code.as_str())
                        && may_be_unmatched(&row.pattern) =>
                {
                    *counts.entry("pattern unmatched").or_default() += 1;
                    continue;
                }
                other => panic!("pattern {:?} on {string:?}: {other:?}", row.pattern),
            }
            *counts.entry("pattern agrees").or_default() += 1;
        }
    }
    // `patternProperties`: a name the evaluator does not match refuses
    // preparation, so those rows are prepared alone.
    let (alone, together): (Vec<_>, Vec<_>) = valid
        .iter()
        .filter(|(index, _)| !excluded.contains(index))
        .copied()
        .partition(|(_, row)| may_be_unmatched(&row.pattern));
    for (_, row) in alone {
        match prepare(
            json!({"patternProperties": {row.pattern.as_str(): false}}),
            ResourceSet::default(),
        ) {
            Err(detail) if UNMATCHED.contains(&detail.code.as_str()) => {
                *counts.entry("patternProperties unmatched").or_default() += 1;
            }
            Err(detail) => panic!("{:?}: {detail:?}", row.pattern),
            Ok(contract) => {
                for (at, string) in row.strings.iter().enumerate() {
                    let value = JsonValue::parse(json!({ string: 0 }).to_string()).unwrap();
                    let fails = matches!(contract.validate(&value), ValueOutcome::Fails { .. });
                    assert_eq!(fails, expected(row, at), "{:?} on {string:?}", row.pattern);
                    *counts.entry("patternProperties agrees").or_default() += 1;
                }
            }
        }
    }
    let (contract, _) = combined(
        &together,
        |p| json!({"patternProperties": {p: false}}),
        "patternProperties",
        |index| panic!("{:?} refused as invalid", rows[index].pattern),
    );
    for &(index, row) in &together {
        for (at, string) in row.strings.iter().enumerate() {
            let value =
                JsonValue::parse(json!({ member(index): { string: 0 } }).to_string()).unwrap();
            let fails = matches!(contract.validate(&value), ValueOutcome::Fails { .. });
            assert_eq!(fails, expected(row, at), "{:?} on {string:?}", row.pattern);
            *counts.entry("patternProperties agrees").or_default() += 1;
        }
    }
    // An `unevaluatedProperties` keyword reads the same names as evaluated
    // when they match.
    let (contract, _) = combined(
        &together,
        |p| json!({"patternProperties": {p: true}, "unevaluatedProperties": false}),
        "patternProperties",
        |index| panic!("{:?} refused as invalid", rows[index].pattern),
    );
    for &(index, row) in &together {
        for (at, string) in row.strings.iter().enumerate() {
            let value =
                JsonValue::parse(json!({ member(index): { string: 0 } }).to_string()).unwrap();
            let satisfies = matches!(contract.validate(&value), ValueOutcome::Satisfies);
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
    // through both keywords; 135 hold constructs the evaluator does not match.
    // Then 130 fixed patterns with sets that meet across the surrogate gap,
    // with 13 strings each, and 128 with word-boundary assertions, with 15
    // strings each; the 64 holding `\B` are not matched.
    assert_eq!(
        counts,
        [
            ("invalid refused", 610),
            ("pattern agrees", 9_106),
            ("pattern unmatched", 2_040),
            ("patternProperties agrees", 9_106),
            ("patternProperties unmatched", 199),
            ("unevaluated marking agrees", 9_106),
        ]
        .into()
    );
}

// The regex crate's `is_match` answers no for `\x{E9}|(?-u:\B)` on "aéa",
// whose first branch matches, because the non-word boundary can hold inside a
// multi-byte character. So `\B` is not matched: a string that reaches it gives
// no verdict, and a `patternProperties` name holding it refuses preparation.
// `\b` keeps its verdicts.
#[test]
fn a_non_word_boundary_is_not_matched_and_a_word_boundary_is() {
    for (pattern, value) in [
        ("^.*$|\\B", "aéa"),
        ("[^a]|\\B", "a中a"),
        ("\\W|\\B", "a😀a"),
        ("\\B", "abc"),
    ] {
        let contract = prepare(json!({"pattern": pattern}), ResourceSet::default()).unwrap();
        let value = JsonValue::parse(json!(value).to_string()).unwrap();
        let ValueOutcome::NoVerdict { detail } = contract.validate(&value) else {
            panic!("{pattern}")
        };
        assert_eq!(detail.code, "pattern-non-word-boundary", "{pattern}");
        assert_eq!(detail.reason, NoVerdictReason::UnsupportedCapability);
        for schema in [
            json!({"patternProperties": {pattern: {"type": "integer"}}}),
            json!({"patternProperties": {pattern: true}, "unevaluatedProperties": false}),
        ] {
            let Err(detail) = prepare(schema, ResourceSet::default()) else {
                panic!("{pattern}")
            };
            assert_eq!(detail.code, "pattern-non-word-boundary", "{pattern}");
        }
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
        let contract = prepare(json!({"pattern": pattern}), ResourceSet::default()).unwrap();
        let outcome = contract.validate(&JsonValue::parse(json!(value).to_string()).unwrap());
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
    for (first, second) in [("\\p{L}", wide.as_str()), (wide.as_str(), "\\p{L}")] {
        let contract = prepare(
            json!({"properties": {"a": {"pattern": first}, "b": {"pattern": second}, "c": {"pattern": "\\Bx"}}}),
            ResourceSet::default(),
        )
        .unwrap();
        for (member, pattern) in [("a", first), ("b", second), ("c", "\\Bx")] {
            let value = JsonValue::parse(json!({ member: "x" }).to_string()).unwrap();
            let ValueOutcome::NoVerdict { detail } = contract.validate(&value) else {
                panic!("{member}")
            };
            let expected = if pattern.starts_with("\\p") {
                "unicode-property-matching"
            } else if pattern.starts_with("\\B") {
                "pattern-non-word-boundary"
            } else {
                "pattern-size-limit"
            };
            assert_eq!(detail.code, expected, "{member}");
        }
    }
}
