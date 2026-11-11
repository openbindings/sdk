//! Compiled and independently prepared Original evaluation guarantees.
use super::*;
use openbindings_internal_schema::preparation::{Attempt, EnvironmentRequest};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
static HEALTHY: AtomicBool = AtomicBool::new(false);
mod decimal_scratch;
mod expansion;
mod generated;
mod literal_scratch;
mod regex_search;

/// Independent preparation of the pinned current original-schema Plan. It does
/// not construct a compiled owner and then disable it.
struct Original(Limits);
impl SchemaEvaluator for Original {
    type Validator = DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Attempt::run(
            limits.max_sdk_work_units,
            limits.max_sdk_reservation_bytes,
            &|| control.is_cancelled(),
            |attempt| {
                control.check()?;
                let environment = request.environment_with_control(
                    limits.max_schema_nodes,
                    limits.max_identity_bytes,
                    attempt,
                )?;
                let plan = semantic::Plan::new(environment, self.0.clone(), *limits, attempt)?;
                control.check()?;
                Ok(DefaultValidator {
                    plan: Arc::new(plan),
                    compiled: compiled::Lane::Original("independent-original"),
                })
            },
        )
        .map_err(PreparationFailure::from)?
    }
}
fn prepare_document<E: SchemaEvaluator<Validator = DefaultValidator>>(
    document: &str,
    resources: &ExternalSchemas,
    evaluator: &E,
    limits: &PreparationLimits,
) -> Result<DefaultValidator, PreparationFailure> {
    let selected = Document::parse(document)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant qualification input")
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap();
    evaluator.prepare(
        EvaluationRequest {
            root: SchemaRoot::Contained(&selected),
            external_schemas: resources,
        },
        limits,
        &AtomicBool::new(false),
    )
}
fn document(schema: &str) -> String {
    format!(r##"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"##)
}
fn owners(
    schema: &str,
    resources: &ExternalSchemas,
    limits: Limits,
    preparation: PreparationLimits,
) -> (DefaultValidator, DefaultValidator) {
    let resources = ExternalSchemas::new(resources.iter().cloned().chain(std::iter::once(
        ExternalSchema {
            uri: "https://qualification.invalid/root".into(),
            document: JsonValue::parse(schema).unwrap(),
        },
    )))
    .unwrap();
    let document = document(r#"{"$ref":"https://qualification.invalid/root"}"#);
    let selected = prepare_document(
        &document,
        &resources,
        &DefaultEvaluator::with_limits(limits.clone()),
        &preparation,
    )
    .unwrap_or_else(|failure| panic!("{}", serde_json::to_string(&failure).unwrap()));
    let original = prepare_document(&document, &resources, &Original(limits), &preparation)
        .unwrap_or_else(|failure| panic!("{}", serde_json::to_string(&failure).unwrap()));
    (selected, original)
}
fn serialized(outcome: ValueOutcome) -> serde_json::Value {
    serde_json::to_value(outcome).unwrap()
}
fn compare(
    selected: &DefaultValidator,
    original: &DefaultValidator,
    value: &JsonValue,
) -> (&'static str, [usize; 3]) {
    compiled::entries::take();
    for detail in [Detail::Outcome, Detail::Explain] {
        let actual = selected.evaluate(value, detail, &HEALTHY);
        let expected = original.evaluate(value, detail, &HEALTHY);
        assert_eq!(
            serialized(actual),
            serialized(expected),
            "evaluation outcomes must agree for {detail:?}"
        );
    }
    let counts = compiled::entries::take();
    (
        if counts[2] == 1 {
            "compiled"
        } else {
            "original"
        },
        counts,
    )
}

#[test]
fn compiled_and_original_agree_on_exact_numbers_text_and_references() {
    let cases = [
        (r##"{"type":"integer"}"##, "1.0"),
        (r##"{"type":"integer"}"##, "1e400"),
        (r##"{"type":"integer"}"##, "1e-200000"),
        (r##"{"type":"integer"}"##, "-0e999999999999999999999999"),
        (r##"{"maximum":9007199254740992}"##, "9007199254740993"),
        (
            r##"{"minimum":9223372036854775807}"##,
            "9223372036854775808",
        ),
        (
            r##"{"minimum":-9223372036854775808}"##,
            "-9223372036854775809",
        ),
        (r##"{"minimum":1e400}"##, "9e399"),
        (r##"{"exclusiveMaximum":1e400}"##, "10e399"),
        (r##"{"maximum":-0}"##, "0e99999999999999999999999"),
        (r##"{"multipleOf":0.1}"##, "0.3"),
        (r##"{"multipleOf":0.1}"##, "0.30000000000000004"),
        (r##"{"multipleOf":1e-400}"##, "2e-400"),
        (r##"{"multipleOf":3}"##, "1e19990"),
        (r##"{"multipleOf":3}"##, "1e20001"),
        (r##"{"multipleOf":3}"##, "0e9999999999999999999999"),
        (r##"{"multipleOf":3}"##, "123456789012345678901234567890"),
        (
            r##"{"const":{"n":[1e400,-0,9007199254740993]}}"##,
            r##"{"n":[10e399,0.0,9.007199254740993e15]}"##,
        ),
        (r##"{"const":9007199254740993}"##, "9007199254740992"),
        (r##"{"enum":[{"n":1.0},1e400]}"##, "10e399"),
        (r##"{"uniqueItems":true}"##, "[0.1,0.10]"),
        (
            r##"{"uniqueItems":true}"##,
            "[9007199254740992,9007199254740993]",
        ),
        (
            r##"{"uniqueItems":true}"##,
            r##"[{"n":1e400},{"n":10e399}]"##,
        ),
        (r##"{"type":"string"}"##, r##""\ud800""##),
        (r##"{"type":"object"}"##, r##"{"\ud800":1}"##),
        (r##"{"type":"object"}"##, r##"{"x":1,"x":2}"##),
        (r##"{"pattern":"^[A-Za-z]+$"}"##, r##""Matthew""##),
        (r##"{"pattern":"(?<é>a)"}"##, r##""a""##),
        (
            r##"{"$defs":{"n":{"type":"integer"}},"allOf":[{"$ref":"#/$defs/n"},{"$ref":"#/$defs/n"}]}"##,
            "42",
        ),
        (
            r##"{"$defs":{"n":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/$defs/n"}}]}},"$ref":"#/$defs/n"}"##,
            "[[42]]",
        ),
        (r##"{"$ref":"https://absent.invalid/schema"}"##, "42"),
        (r##"{"$ref":"#/$defs/missing"}"##, "42"),
        (
            r##"{"anyOf":[true,{"$ref":"https://absent.invalid/schema"}]}"##,
            "42",
        ),
        (
            r##"{"$dynamicAnchor":"node","properties":{"child":{"$dynamicRef":"#node"}}}"##,
            r##"{"child":{}}"##,
        ),
        (
            r##"{"allOf":[{"properties":{"x":true}}],"unevaluatedProperties":false}"##,
            r##"{"x":1}"##,
        ),
    ];
    for (schema, text) in cases {
        compiled::entries::take();
        let (selected, original) = owners(
            schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        compiled::entries::take();
        compare(&selected, &original, &JsonValue::parse(text).unwrap());
    }
    // Unpaired content outside the selected subtree must not disqualify it.
    let (selected, original) = owners(
        r##"{"type":"integer"}"##,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    let wrapper = JsonValue::parse(r##"{"outside":"\ud800","selected":42}"##).unwrap();
    compare(
        &selected,
        &original,
        &wrapper.get("selected").unwrap().to_owned(),
    );
}

const RECURSIVE_INTEGER_ARRAY: &str = r##"{"$defs":{"node":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/$defs/node"}}]},"unused":{"const":false}},"$ref":"#/$defs/node"}"##;
const ORDINARY_NAMED: &str = r##"{"type":"object","properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["name","children"],"additionalProperties":false}"##;
const ORDINARY_ENUM: &str = r##"{"type":"object","properties":{"name":{"type":"string"},"kind":{"enum":["branch","leaf"]},"children":{"type":"array","items":{"$ref":"#"}}},"required":["name","kind","children"],"additionalProperties":false}"##;
const ORDINARY_BINARY: &str = r##"{"type":["object","null"],"properties":{"left":{"$ref":"#"},"right":{"$ref":"#"}},"required":["left","right"],"additionalProperties":false}"##;
const PRODUCTIVE_MINIMUM: &str = r##"{"type":["object","null"],"properties":{"value":{"type":"integer","minimum":0},"left":{"$ref":"#"},"right":{"$ref":"#"}},"required":["value","left","right"],"additionalProperties":false}"##;
const PRODUCTIVE_ADDITIONAL: &str = r##"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":{"$ref":"#"}}"##;
const PRODUCTIVE_UNION: &str = r##"{"oneOf":[{"type":"object","properties":{"kind":{"const":"leaf"},"value":{"type":"integer"}},"required":["kind","value"],"additionalProperties":false},{"type":"object","properties":{"kind":{"const":"branch"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["kind","children"],"additionalProperties":false}]}"##;
fn nested(leaf: &str, depth: usize) -> JsonValue {
    JsonValue::parse(format!("{}{leaf}{}", "[".repeat(depth), "]".repeat(depth))).unwrap()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only compiled recursion boundary stack qualification"
)]
fn recursive_compiled_boundary_runs_on_qualified_native_threads() {
    for stack in [256 * 1024, 1024 * 1024] {
        let (selected, original) = owners(
            RECURSIVE_INTEGER_ARRAY,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(move || {
                let limit = selected.compiled.qualified_depth().unwrap();
                for depth in [limit, limit + 1] {
                    for leaf in ["0", "false"] {
                        assert_eq!(
                            compare(&selected, &original, &nested(leaf, depth)).0,
                            if depth == limit {
                                "compiled"
                            } else {
                                "original"
                            }
                        );
                    }
                }
            })
            .unwrap()
            .join()
            .expect("qualified compiled recursion stack");
    }
}

#[test]
fn compiled_scalar_equality_preserves_an_enclosing_preparation_work_stop() {
    let (selected, original) = owners(
        r#"{"const":"node"}"#,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    let value = JsonValue::parse(r#""node""#).unwrap();
    compare(&selected, &original, &value);
    for poisoned in [false, true] {
        let mut observed = None;
        compiled::entries::take();
        let outer = Attempt::run(0, usize::MAX, &|| false, |attempt| {
            if poisoned {
                let _ = openbindings_internal_schema::Control::charge_work(attempt, 1);
            }
            observed = Some(selected.evaluate(&value, Detail::Outcome, &HEALTHY));
        });
        assert_eq!(outer.unwrap_err().code, "preparation-sdk-work-units");
        assert!(
            matches!(observed, Some(ValueOutcome::NoVerdict { detail }) if detail.reason == NoVerdictReason::LimitExceeded && detail.code == "evaluation-sdk-work-units"),
            "a stopped exact predicate cannot publish a provisional Boolean"
        );
        compare(&selected, &original, &value);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only ordinary recursion stack qualification"
)]
fn ordinary_recursive_boundaries_run_on_qualified_native_threads() {
    let schemas = [
        ("named", ORDINARY_NAMED),
        ("enum", ORDINARY_ENUM),
        ("binary", ORDINARY_BINARY),
    ];
    for (name, schema) in schemas {
        for stack in [256 * 1024, 1024 * 1024] {
            let (selected, original) = owners(
                schema,
                &ExternalSchemas::default(),
                Limits::default(),
                PreparationLimits::default(),
            );
            let limit = selected.compiled.qualified_depth().unwrap();
            // Linear chains reach the value-depth boundary without constructing
            // an exponentially large balanced tree. Parse before the small thread.
            let steps = if name == "binary" {
                limit
            } else {
                (limit - 1) / 2
            };
            let mut values = Vec::new();
            for beyond in [false, true] {
                for failing in [false, true] {
                    let mut text = if name == "binary" {
                        if failing { "42" } else { "null" }.to_owned()
                    } else {
                        format!(
                            r#"{{"name":{},"kind":{},"children":[]}}"#,
                            if failing && name == "named" {
                                "42"
                            } else {
                                "\"node\""
                            },
                            if failing && name == "enum" {
                                "42"
                            } else {
                                "\"leaf\""
                            }
                        )
                    };
                    if name == "named" {
                        text = text.replace(r#","kind":"leaf""#, "");
                    }
                    for _ in 0..steps + usize::from(beyond) {
                        text = if name == "binary" {
                            format!(r#"{{"left":{text},"right":null}}"#)
                        } else if name == "enum" {
                            format!(r#"{{"name":"node","kind":"branch","children":[{text}]}}"#)
                        } else {
                            format!(r#"{{"name":"node","children":[{text}]}}"#)
                        };
                    }
                    values.push((beyond, failing, JsonValue::parse(&text).unwrap()));
                }
            }
            std::thread::Builder::new()
                .stack_size(stack)
                .spawn(move || {
                    for (beyond, failing, value) in values {
                        assert_eq!(
                            matches!(
                                selected.evaluate(&value, Detail::Outcome, &HEALTHY),
                                ValueOutcome::Satisfies
                            ),
                            !failing
                        );
                        assert_eq!(
                            compare(&selected, &original, &value).0,
                            if beyond { "original" } else { "compiled" }
                        );
                    }
                })
                .unwrap()
                .join()
                .expect("ordinary recursive boundary stack");
        }
    }
}

#[test]
fn recursive_child_partitions_keep_overlaps_and_alias_multiplicity_on_plan() {
    let cases = [
        (
            r##"{"properties":{"a":{"$ref":"#"},"ab":{"$ref":"#"}}}"##,
            r#"{"a":{},"ab":{}}"#,
        ),
        (
            r##"{"properties":{"α":{"$ref":"#"},"αβ":{"$ref":"#"}}}"##,
            r#"{"α":{},"αβ":{}}"#,
        ),
        (
            r##"{"properties":{"a":{"$ref":"#"}},"patternProperties":{"^b":{"$ref":"#"}}}"##,
            r#"{"a":{},"b1":{}}"#,
        ),
        (
            r##"{"properties":{"a":{"$ref":"#"}},"patternProperties":{"^a":{"$ref":"#"}}}"##,
            r#"{"a":{}}"#,
        ),
        (
            r##"{"patternProperties":{"^a":{"$ref":"#"},"^ab":{"$ref":"#"}}}"##,
            r#"{"ab":{}}"#,
        ),
        (
            r##"{"patternProperties":{"^a":{"$ref":"#"},"^b":{"$ref":"#"}}}"##,
            r#"{"a":{},"b":{}}"#,
        ),
        (
            r##"{"patternProperties":{"^a+$":{"$ref":"#"}}}"##,
            r#"{"aaa":{}}"#,
        ),
        (
            r##"{"prefixItems":[{"$ref":"#"},{"$ref":"#"}],"items":{"$ref":"#"}}"##,
            "[[],[],[]]",
        ),
        (
            r##"{"items":{"$ref":"#"},"contains":{"$ref":"#"},"minContains":0}"##,
            "[[]]",
        ),
        (
            r##"{"allOf":[{"properties":{"a":{"$ref":"#"}}},{"properties":{"a":{"$ref":"#"}}}]}"##,
            r#"{"a":{}}"#,
        ),
    ];
    for (schema, text) in cases {
        compiled::entries::take();
        let (selected, original) = owners(
            schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        compare(&selected, &original, &JsonValue::parse(text).unwrap());
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only productive scalar recursion stack qualification"
)]
fn productive_scalar_recursive_boundaries_fit_qualified_threads() {
    let minimum = PRODUCTIVE_MINIMUM;
    let additional = PRODUCTIVE_ADDITIONAL;
    let union = PRODUCTIVE_UNION;
    for (name, schema) in [
        ("minimum", minimum),
        ("additional", additional),
        ("union", union),
    ] {
        for stack in [256 * 1024, 1024 * 1024] {
            for failing in [false, true] {
                let (selected, original) = owners(
                    schema,
                    &ExternalSchemas::default(),
                    Limits::default(),
                    PreparationLimits::default(),
                );
                let limit = selected.compiled.qualified_depth().unwrap();
                let mut values = Vec::new();
                let levels = match name {
                    "minimum" => limit,
                    "additional" => limit - 1,
                    _ => (limit - 1) / 2,
                };
                for beyond in [false, true] {
                    let mut text = match name {
                        "minimum" => "null".to_owned(),
                        "additional" => if failing {
                            r#"{"name":42}"#
                        } else {
                            r#"{"name":"leaf"}"#
                        }
                        .to_owned(),
                        _ => format!(
                            r#"{{"kind":"leaf","value":{}}}"#,
                            if failing { "false" } else { "0" }
                        ),
                    };
                    for i in 0..levels + usize::from(beyond) {
                        text = match name {
                            "minimum" => format!(
                                r#"{{"value":{},"left":{text},"right":null}}"#,
                                if i == 0 && failing { -1 } else { 0 }
                            ),
                            "additional" => format!(r#"{{"name":"node","next":{text}}}"#),
                            _ => format!(r#"{{"kind":"branch","children":[{text}]}}"#),
                        };
                    }
                    values.push((beyond, JsonValue::parse(text).unwrap()));
                }
                std::thread::Builder::new()
                    .stack_size(stack)
                    .spawn(move || {
                        for (beyond, value) in values {
                            assert_eq!(
                                matches!(
                                    selected.evaluate(&value, Detail::Outcome, &HEALTHY),
                                    ValueOutcome::Satisfies
                                ),
                                !failing
                            );
                            assert_eq!(
                                compare(&selected, &original, &value).0,
                                if beyond { "original" } else { "compiled" }
                            );
                        }
                    })
                    .unwrap()
                    .join()
                    .expect("qualified productive scalar recursion stack");
            }
        }
    }
}

#[test]
fn recursive_scalar_literals_keep_their_reservation_charge() {
    // Only the acyclic fold discounts allocation-free scalar literals. Under
    // recursion the `const` strings keep their reservation charge, so a tree of
    // plain integers past the estimate stays on Original.
    let schema = r##"{"oneOf":[{"type":"object","properties":{"kind":{"const":"leaf"},"value":{"type":"integer","minimum":0}},"required":["kind","value"],"additionalProperties":false},{"type":"object","properties":{"kind":{"const":"branch"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["kind","children"],"additionalProperties":false}]}"##;
    let (selected, original) = owners(
        schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    for (leaves, lane) in [(100, "compiled"), (1000, "original")] {
        let value = JsonValue::parse(format!(
            r#"{{"kind":"branch","children":[{}]}}"#,
            vec![r#"{"kind":"leaf","value":1}"#; leaves].join(",")
        ))
        .unwrap();
        assert_eq!(compare(&selected, &original, &value).0, lane);
    }
}
