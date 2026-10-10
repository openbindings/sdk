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
            // The unchanged internal Number protocol and SDK checked serializer interoperate.
            let private: private_json::Number = private_json::from_str(token).unwrap();
            assert_eq!(
                JsonValue::from_serializable(&private).unwrap().text(),
                private.to_string()
            );
            let raw: Box<serde_json::value::RawValue> = serde_json::from_str(token).unwrap();
            let private_raw: Box<private_json::value::RawValue> =
                private_json::from_str(token).unwrap();
            assert_eq!(private_json::to_string(&raw).unwrap(), token);
            assert_eq!(serde_json::to_string(&private_raw).unwrap(), token);
            // Raw JSON remains forbidden at the ordinary checked-data admission boundary.
            assert_eq!(
                JsonValue::from_serializable(&raw).unwrap_err().kind(),
                openbindings::ValueConversionErrorKind::UnsupportedRepresentation
            );
            #[cfg(feature = "ap")]
            {
                let public: Number = serde_json::from_str(token).unwrap();
                let ordinary = serde_json::json!({"n": public});
                let admitted = JsonValue::from_serializable(&ordinary).unwrap();
                assert_eq!(
                    admitted.get("n").unwrap().number_text(),
                    Some(private.to_string().as_str())
                );
                assert_eq!(
                    private_json::to_string(&ordinary).unwrap(),
                    serde_json::to_string(&ordinary).unwrap()
                );
                assert_eq!(
                    serde_json::to_string(&private).unwrap(),
                    private.to_string()
                );
                let roundtrip: Number =
                    serde_json::from_str(&private_json::to_string(&public).unwrap()).unwrap();
                assert_eq!(roundtrip, public);
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
    fn public_custom_evaluator_and_projection_use_sdk_values() {
        use openbindings::*;
        use std::sync::Arc;
        struct ExactConstant;
        struct Prepared(JsonValue);
        impl SchemaEvaluator for ExactConstant {
            fn prepare(
                &self,
                request: &SchemaRequest,
                control: &WorkControl,
            ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
                let program: EvaluationProgram = request.evaluation_program(control)?;
                let _: &String = &program.entry_uri;
                let resources: &Vec<SchemaResource> = &program.resources;
                assert!(!resources.is_empty());
                for resource in resources {
                    let document: &JsonValue = &resource.document;
                    let encoded = serde_json::to_string(document).unwrap();
                    let decoded: JsonValue = serde_json::from_str(&encoded).unwrap();
                    assert_eq!(decoded.text(), document.text());
                }
                let entry = resources
                    .iter()
                    .find(|resource| resource.uri == program.entry_uri)
                    .unwrap();
                let reference = entry.document.get("$ref").unwrap();
                let location: SchemaLocation = program
                    .original_location(reference.as_str().unwrap())
                    .unwrap();
                assert_eq!(location.pointer, request.entry_location().pointer);
                let constant: JsonValue = request.entry().get("const").unwrap().to_owned();
                assert_eq!(constant.text(), "1e400");
                Ok(Arc::new(Prepared(constant)))
            }
        }
        impl PreparedSchema for Prepared {
            fn validate(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome {
                control.check().unwrap();
                // This finite signature witness only calls the supported exact constant.
                assert_eq!(value.semantic_eq(&self.0), Some(true));
                ValueOutcome::Satisfies
            }
        }
        let document = ParsedDocument::parse(
            r#"{"openbindings":"0.2.0","operations":{"read":{"input":{"const":1e400}}}}"#,
        )
        .unwrap();
        let context = document
            .value_contracts(Arc::new(ExactConstant), ResourceSet::default())
            .unwrap();
        let ContractPreparation::Ready(contract) = context.prepare("read", Side::Input) else {
            panic!("custom evaluator must prepare")
        };
        assert!(matches!(
            contract.validate(&JsonValue::parse("1e400").unwrap()),
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
