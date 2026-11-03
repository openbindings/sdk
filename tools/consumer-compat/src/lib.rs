//! External consumer: its serde_json dependency must keep its selected features.
#![allow(dead_code)]

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::{Number, Value};
    use std::collections::BTreeMap;

    #[derive(Debug, Deserialize)]
    struct Numbers {
        n: f64,
    }
    #[derive(Debug, Deserialize)]
    struct FlatTyped {
        #[serde(flatten)]
        payload: Numbers,
    }
    #[derive(Debug, Deserialize)]
    struct FlatValue {
        #[serde(flatten)]
        extras: BTreeMap<String, Value>,
    }
    #[derive(Debug, Deserialize)]
    struct FlatNumber {
        #[serde(flatten)]
        extras: BTreeMap<String, Number>,
    }
    #[derive(Debug, Deserialize)]
    struct FlatUnsigned {
        #[serde(flatten)]
        extras: BTreeMap<String, u64>,
    }
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum Numeric {
        Unsigned(u64),
        Float(f64),
    }
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum ExactNumeric {
        Number(Number),
        Text(String),
    }
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum Record {
        Numeric(Numbers),
        Text { n: String },
    }

    #[test]
    fn consumers_keep_their_selected_numeric_buffering_behavior() {
        for token in [
            "42",
            "-42",
            "1.25",
            "1e3",
            "123456789012345678901234567890",
            "0.123456789012345678901234567890",
        ] {
            let object = format!("{{\"n\":{token}}}");
            assert!(serde_json::from_str::<f64>(token).is_ok());
            assert!(serde_json::from_str::<Numbers>(&object).is_ok());
            assert!(serde_json::from_str::<FlatValue>(&object).is_ok());
            assert!(serde_json::from_str::<FlatNumber>(&object).is_ok());
            assert!(serde_json::from_str::<ExactNumeric>(token).is_ok());
            let buffered_numeric = !cfg!(feature = "ap") || matches!(token, "42" | "-42");
            assert_eq!(
                serde_json::from_str::<FlatTyped>(&object).is_ok(),
                buffered_numeric,
                "flatten {token}"
            );
            assert_eq!(
                serde_json::from_str::<Numeric>(token).is_ok(),
                buffered_numeric,
                "untagged {token}"
            );
            assert_eq!(
                serde_json::from_str::<Record>(&object).is_ok(),
                buffered_numeric,
                "record {token}"
            );
        }
        assert!(serde_json::from_str::<ExactNumeric>("\"ordinary\"").is_ok());
        assert!(serde_json::from_str::<Record>(r#"{"n":"ordinary"}"#).is_ok());
        let integer: FlatUnsigned = serde_json::from_str(r#"{"n":42}"#).unwrap();
        assert_eq!(integer.extras["n"], 42);
        assert_eq!(
            serde_json::from_str::<Number>("1e400").is_ok(),
            cfg!(feature = "ap")
        );
    }

    // Compare fresh observations across actual Cargo feature graphs. The verifier
    // pairs this matrix with evaluator/ap/preserve_order/conversion-check runs.
    // Historical budget cutoffs and diagnostic prose are not SDK compatibility
    // requirements. Default-funded ordinary cases retain independent semantic answers.
    #[cfg(feature = "evaluator")]
    #[test]
    fn serde_json_member_order_never_changes_preparation_or_verdicts() {
        use openbindings::{Document, JsonValue};
        use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
        let cases: Value = serde_json::from_str(include_str!("member-order-cases.json")).unwrap();
        let mut observations = Vec::new();
        for case in cases.as_array().unwrap() {
            let id = case["id"].as_str().unwrap();
            let document = Document::parse(case["document"].as_str().unwrap()).unwrap();
            let proof = document
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant case");
            let operation = proof.resolve_operation("t").unwrap();
            let contract = operation.input().unwrap();
            let values: Vec<JsonValue> = case["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| JsonValue::parse(value.as_str().unwrap()).unwrap())
                .collect();
            for run in case["runs"].as_array().unwrap() {
                let limits: Limits = serde_json::from_value(run["limits"].clone()).unwrap();
                let funded = run["limits"].as_object().unwrap().is_empty();
                let observed = match contract.prepare(
                    &DefaultEvaluator::with_limits(limits),
                    &openbindings::ExternalSchemas::default(),
                    &openbindings::PreparationLimits::default(),
                ) {
                    Err(refusal) => {
                        assert!(
                            !funded,
                            "{id}: ordinary case must prepare with default limits"
                        );
                        assert!(
                            matches!(
                                refusal.kind,
                                openbindings::PreparationFailureKind::Limit { .. }
                            ),
                            "{id}"
                        );
                        serde_json::json!({"prepare": refusal})
                    }
                    Ok(prepared) => {
                        let answers: Vec<Value> = values
                            .iter()
                            .enumerate()
                            .map(|(index, value)| {
                                let cheap = serde_json::to_value(prepared.evaluate(value)).unwrap();
                                let explained =
                                    serde_json::to_value(prepared.explain(value)).unwrap();
                                for answer in [&cheap, &explained] {
                                    match answer["status"].as_str().unwrap() {
                                        "satisfies" | "fails" | "undefined" => assert_eq!(
                                            answer["status"],
                                            case["required_statuses"][index],
                                            "{id}: {}",
                                            value.text()
                                        ),
                                        "no-verdict" => {
                                            let reason =
                                                answer["detail"]["reason"].as_str().unwrap();
                                            let capability =
                                                case["capability_reasons"][index].as_str();
                                            assert!(
                                                (!funded && reason == "limit-exceeded")
                                                    || capability == Some(reason),
                                                "{id}: unexpected incomplete result {answer}"
                                            );
                                        }
                                        status => panic!("unexpected status: {status}"),
                                    }
                                }
                                serde_json::json!([cheap, explained])
                            })
                            .collect();
                        serde_json::json!({"prepare": "prepared", "values": answers})
                    }
                };
                observations.push(
                    serde_json::json!({"id": id, "limits": run["limits"], "observed": observed}),
                );
            }
        }
        assert!(!observations.is_empty());
        if let Some(path) = std::env::var_os("OB_CONSUMER_OBSERVATIONS") {
            std::fs::write(path, serde_json::to_vec(&observations).unwrap()).unwrap();
        }
    }

    #[cfg(feature = "sdk")]
    #[test]
    fn exact_values_cross_package_serde_boundaries() {
        use openbindings::JsonValue;
        let source = r#"{"duplicate":1,"duplicate":2,"text":"\ud800","n":1e400}"#;
        let exact = JsonValue::parse(source).unwrap();
        assert_eq!(serde_json::to_string(&exact).unwrap(), source);
        assert_eq!(
            serde_json::from_str::<JsonValue>(source).unwrap().text(),
            source
        );
        for token in [
            "42",
            "9007199254740993",
            "123456789012345678901234567890",
            "0.123456789012345678901234567890",
            "1e3",
            "1e400",
            "1e99999999999999999999999999999999999",
        ] {
            // Parsing preserves source spelling independently of consumer JSON features.
            let exact = JsonValue::parse(token).unwrap();
            assert_eq!(exact.text(), token);
            assert_eq!(serde_json::to_string(&exact).unwrap(), token);
            let decoded: JsonValue = serde_json::from_str(token).unwrap();
            assert_eq!(decoded.text(), token);
            let raw: Box<serde_json::value::RawValue> = serde_json::from_str(token).unwrap();
            assert_eq!(serde_json::to_string(&raw).unwrap(), token);
            assert_eq!(
                JsonValue::from_serializable(&raw).unwrap_err().kind(),
                openbindings::ValueConversionErrorKind::UnsupportedRepresentation
            );
            // Consumer-enabled AP uses the same identity. Preserve its emitted
            // Number protocol; spelling already normalized by serde is not recovered.
            #[cfg(feature = "ap")]
            {
                let number: Number = serde_json::from_str(token).unwrap();
                let expected = number.to_string();
                assert_eq!(
                    JsonValue::from_serializable(&number).unwrap().text(),
                    expected
                );
                let ordinary = serde_json::json!({"n": number});
                let admitted = JsonValue::from_serializable(&ordinary).unwrap();
                assert_eq!(
                    admitted.get("n").unwrap().number_text(),
                    Some(expected.as_str())
                );
            }
        }
        let ordinary: Value = serde_json::from_str(r#"{"n":1.25,"i":9007199254740993}"#).unwrap();
        let admitted = JsonValue::from_serializable(&ordinary).unwrap();
        assert_eq!(admitted.get("n").unwrap().number_text(), Some("1.25"));
        assert_eq!(
            admitted.get("i").unwrap().number_text(),
            Some("9007199254740993")
        );
    }

    #[cfg(feature = "sdk")]
    #[test]
    fn public_custom_evaluator_and_original_environment_use_sdk_values() {
        use openbindings::*;
        struct ExactConstant;
        struct Prepared(JsonValue);
        impl SchemaEvaluator for ExactConstant {
            type Validator = Prepared;
            fn prepare(
                &self,
                request: EvaluationRequest<'_>,
                limits: &PreparationLimits,
                control: &dyn WorkControl,
            ) -> Result<Prepared, PreparationFailure> {
                let environment = request.environment(limits, control)?;
                let root = environment.root();
                let document: JsonValue = root.value();
                let encoded = serde_json::to_string(&document).unwrap();
                let decoded: JsonValue = serde_json::from_str(&encoded).unwrap();
                assert_eq!(decoded.text(), document.text());
                assert_eq!(root.origin().pointer, "/operations/read/input");
                assert_eq!(root.origin().source_uri, None);
                let constant: JsonValue = document.get("const").unwrap().to_owned();
                assert_eq!(constant.text(), "1e400");
                Ok(Prepared(constant))
            }
        }
        impl SchemaValidator for Prepared {
            fn evaluate(
                &self,
                value: &JsonValue,
                _: Detail,
                control: &dyn WorkControl,
            ) -> ValueOutcome {
                control.check().unwrap();
                // This finite signature witness only calls the supported exact constant.
                assert_eq!(value.semantic_eq(&self.0), Some(true));
                ValueOutcome::Satisfies
            }
        }
        let document = Document::parse(
            r#"{"openbindings":"0.2.0","operations":{"read":{"input":{"const":1e400}}}}"#,
        )
        .unwrap();
        let proof = document
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant document");
        let operation = proof.resolve_operation("read").expect("declared operation");
        let input: Schema = operation.input().expect("declared input");
        let contract: PreparedSchema<Prepared> = input
            .prepare(
                &ExactConstant,
                &openbindings::ExternalSchemas::default(),
                &openbindings::PreparationLimits::default(),
            )
            .expect("custom evaluator must prepare");
        assert!(matches!(
            contract.evaluate(&JsonValue::parse("1e400").unwrap()),
            ValueOutcome::Satisfies
        ));
    }

    #[cfg(feature = "sdk")]
    #[test]
    fn checked_protocol_still_validates_shape_tokens_locations_and_budgets() {
        use openbindings::{JsonLimits, JsonValue, ValueConversionErrorKind};
        use serde::{Serialize, Serializer, ser::SerializeStruct};
        struct Protocol<'a>(&'a str, u8);
        impl Serialize for Protocol<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut state = serializer.serialize_struct("$serde_json::private::Number", 1)?;
                match self.1 {
                    1 => {}
                    2 => state.serialize_field("wrong", self.0)?,
                    3 => {
                        state.serialize_field("$serde_json::private::Number", self.0)?;
                        state.serialize_field("$serde_json::private::Number", self.0)?;
                    }
                    4 => state.serialize_field("$serde_json::private::Number", &7)?,
                    _ => state.serialize_field("$serde_json::private::Number", self.0)?,
                }
                state.end()
            }
        }
        for protocol in [
            Protocol("1", 1),
            Protocol("1", 2),
            Protocol("1", 3),
            Protocol("1", 4),
            Protocol("NaN", 0),
            Protocol("true", 0),
            Protocol("1,\"injected\":false", 0),
            Protocol(" 1 ", 0),
        ] {
            let error =
                JsonValue::from_serializable(&BTreeMap::from([("n", protocol)])).unwrap_err();
            assert_eq!(
                error.kind(),
                ValueConversionErrorKind::UnsupportedRepresentation
            );
            assert_eq!(error.pointer(), Some("/n"));
        }
        let limits = JsonLimits {
            max_bytes: 5,
            ..Default::default()
        };
        assert_eq!(
            JsonValue::from_serializable_with_limits(&Protocol("123456", 0), limits)
                .unwrap_err()
                .kind(),
            ValueConversionErrorKind::Limit
        );
        let limits = JsonLimits {
            max_bytes: 1,
            max_nodes: 1,
            max_depth: 0,
        };
        assert_eq!(
            JsonValue::from_serializable_with_limits(&Protocol("7", 0), limits)
                .unwrap()
                .text(),
            "7"
        );
    }
}
