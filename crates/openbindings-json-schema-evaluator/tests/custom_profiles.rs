//! Admission uses the selected original meta-schema, including restrictions
//! beyond its vocabulary declaration. Its own profile is independent of the
//! profile it declares for consumer schemas.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator};

const META: &str = "https://custom.test/dir/meta";
const SCHEMA: &str = "https://custom.test/schema";
const VOCABULARIES: &str = r#""https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/applicator":true,"https://json-schema.org/draft/2020-12/vocab/validation":true,"https://json-schema.org/draft/2020-12/vocab/unevaluated":true"#;

fn prepare(
    vocabularies: &str,
    meta: &str,
    consumer: &str,
    extra: &[(&str, &str)],
) -> Result<PreparedSchema<DefaultValidator>, PreparationFailure> {
    let schema = Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{{"$ref":"{SCHEMA}"}}}}}}"#))
        .unwrap().validate().unwrap().conformant().unwrap().schema("entry").unwrap();
    let mut resources = vec![
        ExternalSchema { uri: META.into(), document: JsonValue::parse(format!(r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{{{vocabularies}}},{meta}}}"#)).unwrap() },
        ExternalSchema { uri: SCHEMA.into(), document: JsonValue::parse(format!(r#"{{"$schema":"{META}",{consumer}}}"#)).unwrap() },
    ];
    resources.extend(extra.iter().map(|(uri, text)| ExternalSchema {
        uri: (*uri).into(),
        document: JsonValue::parse(text).unwrap(),
    }));
    schema.prepare(
        &DefaultEvaluator::new(),
        &ExternalSchemas::new(resources).unwrap(),
        &PreparationLimits::default(),
    )
}
fn invalid(result: Result<PreparedSchema<DefaultValidator>, PreparationFailure>) {
    assert_eq!(
        result.unwrap_err().kind,
        PreparationFailureKind::InvalidExternalSchema
    );
}
fn verdict(prepared: &PreparedSchema<DefaultValidator>, value: &str, succeeds: bool) {
    let outcome = prepared.evaluate(&JsonValue::parse(value).unwrap());
    assert!(
        if succeeds {
            matches!(outcome, ValueOutcome::Satisfies)
        } else {
            matches!(outcome, ValueOutcome::Fails { .. })
        },
        "{outcome:?}"
    );
}

#[test]
fn actual_meta_required_and_const_restrictions_gate_the_validator() {
    invalid(prepare(
        VOCABULARIES,
        r#""enum":[]"#,
        r#""type":"integer""#,
        &[],
    ));
    let meta = r#""required":["marker"],"properties":{"marker":{"const":true}}"#;
    invalid(prepare(VOCABULARIES, meta, r#""type":"integer""#, &[]));
    invalid(prepare(
        VOCABULARIES,
        meta,
        r#""type":"integer","marker":false"#,
        &[],
    ));
    let prepared = prepare(VOCABULARIES, meta, r#""type":"integer","marker":true"#, &[]).unwrap();
    verdict(&prepared, "1", true);
    verdict(&prepared, "false", false);
}

#[test]
fn a_permissive_custom_meta_can_admit_an_empty_type_union() {
    let prepared = prepare(VOCABULARIES, r#""type":"object""#, r#""type":[]"#, &[]).unwrap();
    for value in ["null", "false", "0", r#""text""#, "[]", "{}"] {
        verdict(&prepared, value, false);
    }
    let standard = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"type":[]}}}"#,
    )
    .unwrap();
    assert!(standard.validate().unwrap().conformant().is_none());
}

#[test]
fn meta_uses_its_own_profile_while_omitted_consumer_keywords_are_data() {
    let core = r#""https://json-schema.org/draft/2020-12/vocab/core":true"#;
    let meta = r#""not":{"required":["forbidden"]}"#;
    invalid(prepare(core, meta, r#""forbidden":true"#, &[]));
    let prepared = prepare(
        core,
        meta,
        r#""type":123,"properties":"ordinary data""#,
        &[],
    )
    .unwrap();
    verdict(&prepared, "null", true);
    verdict(&prepared, r#"{"anything":true}"#, true);
}

#[test]
fn original_meta_dependent_and_unevaluated_constraints_are_active() {
    let dependent = r#""dependentSchemas":{"flag":{"required":["marker"]}}"#;
    invalid(prepare(VOCABULARIES, dependent, r#""flag":true"#, &[]));
    prepare(VOCABULARIES, dependent, r#""flag":true,"marker":1"#, &[]).unwrap();
    let unevaluated =
        r#""allOf":[{"properties":{"$schema":true,"type":true}}],"unevaluatedProperties":false"#;
    invalid(prepare(
        VOCABULARIES,
        unevaluated,
        r#""type":"integer","extra":1"#,
        &[],
    ));
    prepare(VOCABULARIES, unevaluated, r#""type":"integer""#, &[]).unwrap();
}

#[test]
fn recursive_meta_validation_retains_original_dynamic_scope() {
    let meta = r##""$dynamicAnchor":"meta","type":["object","boolean"],"properties":{"properties":{"additionalProperties":{"$dynamicRef":"#meta"}}},"not":{"required":["forbidden"]}"##;
    invalid(prepare(
        VOCABULARIES,
        meta,
        r#""properties":{"bad":{"forbidden":true}}"#,
        &[],
    ));
    let prepared = prepare(
        VOCABULARIES,
        meta,
        r#""properties":{"ok":{"type":"integer"}}"#,
        &[],
    )
    .unwrap();
    verdict(&prepared, r#"{"ok":1}"#, true);
    verdict(&prepared, r#"{"ok":false}"#, false);
}

#[test]
fn selected_meta_references_resolve_from_its_original_base() {
    let shape = [(
        "https://custom.test/dir/shape",
        r#"{"required":["marker"]}"#,
    )];
    invalid(prepare(
        VOCABULARIES,
        r#""$ref":"shape""#,
        r#""type":"integer""#,
        &shape,
    ));
    prepare(
        VOCABULARIES,
        r#""$ref":"shape""#,
        r#""type":"integer","marker":1"#,
        &shape,
    )
    .unwrap();
}

#[test]
fn meta_must_itself_be_a_valid_schema_under_its_own_profile() {
    invalid(prepare(
        VOCABULARIES,
        r#""required":false"#,
        r#""type":"integer""#,
        &[],
    ));
    for position in [
        r#""$defs":{"unused":{"type":12}}"#,
        r#""contentSchema":{"type":12}"#,
        r#""then":{"type":12}"#,
        r#""else":{"type":12}"#,
    ] {
        invalid(prepare(VOCABULARIES, position, r#""type":"integer""#, &[]));
    }
    invalid(prepare(
        VOCABULARIES,
        r#""$ref":"shape""#,
        r#""type":"integer""#,
        &[(
            "https://custom.test/dir/shape",
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$defs":{"unused":{"type":12}}}"#,
        )],
    ));
}

#[test]
fn meta_reference_missing_bytes_remain_a_structured_preparation_failure() {
    let failure = prepare(
        VOCABULARIES,
        r#""$ref":"absent""#,
        r#""type":"integer""#,
        &[],
    )
    .unwrap_err();
    assert_eq!(
        failure.kind,
        PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::ResourceUnavailable {
                uri: "https://custom.test/dir/absent".into()
            }
        }
    );
}

#[test]
fn admission_dependency_cycles_do_not_count_as_validated() {
    let failure = prepare(
        VOCABULARIES,
        r#""$ref":"https://custom.test/schema""#,
        r#""type":"integer""#,
        &[],
    )
    .unwrap_err();
    assert_eq!(
        failure.kind,
        PreparationFailureKind::UnsupportedCapability {
            capability: "recursive-meta-obligation".into()
        }
    );
}

#[test]
fn meta_primitives_preserve_exact_utf16_names_values_and_decimal_operands() {
    let meta = r#""required":["\ud800"],"properties":{"\ud800":{"const":"\udc00"},"wide":{"const":1e1000}}"#;
    let consumer = r#""\ud800":"\udc00","wide":10e999,"type":"integer""#;
    let prepared = prepare(VOCABULARIES, meta, consumer, &[]).unwrap();
    verdict(&prepared, "1", true);
    verdict(&prepared, "false", false);
    invalid(prepare(VOCABULARIES, meta, r#""\ud801":"\udc00""#, &[]));
    invalid(prepare(VOCABULARIES, meta, r#""\ud800":"\udc01""#, &[]));
}

#[test]
fn custom_primitive_assertions_read_original_operands_across_arenas() {
    let meta = r#""type":"object""#;
    let cases = [
        (
            r#""type":"integer","minimum":1,"maximum":3,"multipleOf":0.5"#,
            "2",
            "4",
        ),
        (r#""exclusiveMinimum":1,"exclusiveMaximum":3"#, "2", "3"),
        (
            r#""const":{"a":1e1000,"b":"\ud800"}"#,
            r#"{"b":"\ud800","a":10e999}"#,
            r#"{"b":"\ud801","a":1e1000}"#,
        ),
        (r#""enum":["\ud800",1e1000]"#, "10e999", "0"),
        (
            r#""minLength":1,"maxLength":1"#,
            r#""\ud83d\ude00""#,
            r#""ab""#,
        ),
        (
            r#""minItems":1,"maxItems":2,"uniqueItems":true"#,
            "[1,2]",
            "[1,1.0]",
        ),
        (
            r#""minProperties":1,"maxProperties":2,"required":["\ud800"],"dependentRequired":{"\ud800":["other"]}"#,
            r#"{"\ud800":1,"other":2}"#,
            r#"{"\ud800":1}"#,
        ),
    ];
    for (schema, yes, no) in cases {
        let prepared = prepare(VOCABULARIES, meta, schema, &[]).unwrap();
        verdict(&prepared, yes, true);
        verdict(&prepared, no, false);
    }
}

#[test]
fn required_standard_formats_optional_annotation_and_nonstring_values() {
    let pairs: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/required-format-pairs.json")).unwrap();
    for pair in pairs.as_array().unwrap() {
        let name = pair["format"].as_str().unwrap();
        let _ = name;
        let consumer = format!("\"format\":{}", pair["format"]);
        for required in [true, false] {
            let vocab = format!(
                "{VOCABULARIES},\"https://json-schema.org/draft/2020-12/vocab/format-assertion\":{required}"
            );
            let prepared = prepare(&vocab, r#""type":"object""#, &consumer, &[]).unwrap();
            verdict(&prepared, &pair["valid"].to_string(), true);
            verdict(&prepared, &pair["invalid"].to_string(), !required);
            for value in ["null", "false", "1", "1.5", "[]", "{}"] {
                verdict(&prepared, value, true);
            }
        }
    }
}

#[test]
fn unknown_required_formats_refuse_preparation_and_exact_pointer_units_survive() {
    let required = format!(
        "{VOCABULARIES},\"https://json-schema.org/draft/2020-12/vocab/format-assertion\":true"
    );
    let failure = prepare(
        &required,
        r#""type":"object""#,
        r#""format":"unknown-custom-attribute""#,
        &[],
    )
    .unwrap_err();
    assert_eq!(
        failure.kind,
        PreparationFailureKind::UnsupportedCapability {
            capability: "required-format-attribute".into()
        }
    );
    for (name, input) in [
        ("json-pointer", r#""/\ud800""#),
        ("relative-json-pointer", r#""0/\ud800""#),
    ] {
        let prepared = prepare(
            &required,
            r#""type":"object""#,
            &format!("\"format\":\"{name}\""),
            &[],
        )
        .unwrap();
        verdict(&prepared, input, true);
    }
    let annotated = prepare(
        VOCABULARIES,
        r#""type":"object""#,
        r#""format":"unknown-custom-attribute""#,
        &[],
    )
    .unwrap();
    verdict(&annotated, r#""anything""#, true);
    let referenced = prepare(
        &required,
        r#""type":"object""#,
        r##""$ref":"#/$defs/base","$defs":{"base":true},"format":"ipv4""##,
        &[],
    )
    .unwrap();
    verdict(&referenced, r#""127.0.0.1""#, true);
    verdict(&referenced, r#""not-an-address""#, false);
}

#[test]
fn regex_format_matches_independent_no_flags_syntax_controls() {
    let required = format!(
        "{VOCABULARIES},\"https://json-schema.org/draft/2020-12/vocab/format-assertion\":true"
    );
    let prepared = prepare(&required, r#""type":"object""#, r#""format":"regex""#, &[]).unwrap();
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/regex-format-node-controls.json")).unwrap();
    let mut differences = Vec::new();
    for case in cases["cases"].as_array().unwrap() {
        let source = case["source_json"].as_str().unwrap();
        let actual = prepared.evaluate(&JsonValue::parse(source).unwrap());
        let expected = case["valid"].as_bool().unwrap();
        if !matches!(
            (&actual, expected),
            (ValueOutcome::Satisfies, true) | (ValueOutcome::Fails { .. }, false)
        ) {
            differences.push(format!("{source}: expected {expected}, actual {actual:?}"));
        }
    }
    assert!(
        differences.is_empty(),
        "{} mismatches: {}",
        differences.len(),
        differences
            .into_iter()
            .take(20)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn date_time_formats_require_ascii_digits_and_utc_month_boundaries() {
    let required = format!(
        "{VOCABULARIES},\"https://json-schema.org/draft/2020-12/vocab/format-assertion\":true"
    );
    for (name, sample) in [
        ("date", "2000-02-29"),
        ("time", "12:34:56.123+05:30"),
        ("date-time", "2000-02-29T12:34:56.123-05:30"),
    ] {
        let prepared = prepare(
            &required,
            r#""type":"object""#,
            &format!(r#""format":"{name}""#),
            &[],
        )
        .unwrap();
        verdict(&prepared, &serde_json::to_string(sample).unwrap(), true);
        for (at, original) in sample
            .bytes()
            .enumerate()
            .filter(|(_, b)| b.is_ascii_digit())
        {
            for replacement in (0u8..=127).filter(|b| !b.is_ascii_digit()) {
                let mut candidate = sample.as_bytes().to_vec();
                candidate[at] = replacement;
                let candidate = String::from_utf8(candidate).unwrap();
                let outcome = prepared.evaluate(
                    &JsonValue::parse(serde_json::to_string(&candidate).unwrap()).unwrap(),
                );
                assert!(
                    matches!(outcome, ValueOutcome::Fails { .. }),
                    "{name}: {candidate:?} replaced {original} at {at}: {outcome:?}"
                );
            }
        }
    }
    let prepared = prepare(
        &required,
        r#""type":"object""#,
        r#""format":"date-time""#,
        &[],
    )
    .unwrap();
    for (value, valid) in [
        ("1990-12-31T23:59:60Z", true),
        ("1990-12-31T15:59:60-08:00", true),
        ("1991-01-01T00:59:60+01:00", true),
        ("1991-01-01T23:59:60Z", false),
        ("1991-01-02T00:59:60+01:00", false),
        ("1990-12-31T23:59:60+01:00", false),
        ("2000-02-29T12:30:59.5Z", true),
        ("1900-02-29T12:30:59.5Z", false),
    ] {
        verdict(&prepared, &serde_json::to_string(value).unwrap(), valid);
    }
}

#[test]
fn local_meta_checks_do_not_hide_invalid_unused_original_positions() {
    const HELPER: &str = "https://custom.test/standard-helper";
    for dialect in [
        "http://json-schema.org/draft-07/schema",
        "https://json-schema.org/draft/2019-09/schema",
        "https://json-schema.org/draft/2020-12/schema",
    ] {
        for body in [
            r#""definitions":{"unused":{"type":17}}"#,
            r#""then":{"type":17}"#,
            r#""else":{"type":17}"#,
            r#""dependencies":{"unused":{"type":17}}"#,
        ] {
            let helper = format!(r#"{{"$schema":"{dialect}",{body}}}"#);
            invalid(prepare(
                VOCABULARIES,
                &format!(r#""$ref":"{HELPER}""#),
                r#""type":"integer""#,
                &[(HELPER, &helper)],
            ));
        }
        // Identical-looking data is not a syntax position. The complete local
        // extent is still charged, but it does not invent an admission rule.
        let helper = format!(
            r#"{{"$schema":"{dialect}","default":{{"definitions":{{"unused":{{"type":17}}}}}},"unknown":{{"then":{{"type":17}}}}}}"#
        );
        let prepared = prepare(
            VOCABULARIES,
            &format!(r#""$ref":"{HELPER}""#),
            r#""type":"integer""#,
            &[(HELPER, &helper)],
        )
        .unwrap();
        verdict(&prepared, "1", true);
        verdict(&prepared, "false", false);
    }
}
