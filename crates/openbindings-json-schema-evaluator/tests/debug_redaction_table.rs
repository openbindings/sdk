//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Diagnostics};
use std::fmt::Debug;
use std::sync::Arc;
// Keep the independent review's exact credential and pointer probe value.
const SECRET: &str = "REVIEW-BEARER-TOKEN-7391";
const NUMBER: &str = "739173917391";

struct Row {
    label: &'static str,
    text: [String; 4],
}
fn row(label: &'static str, value: &(impl Debug + ?Sized)) -> Row {
    Row {
        label,
        text: [
            format!("{value:?}"),
            format!("{value:#?}"),
            format!("{:?}", Some(value)),
            format!("{:#?}", Some(value)),
        ],
    }
}

fn check_rows(rows: Vec<Row>, expected: usize) {
    assert_eq!(rows.len(), expected);
    for Row { label, text } in &rows {
        for (mode, text) in ["ordinary", "pretty", "nested", "nested pretty"]
            .into_iter()
            .zip(text)
        {
            for marker in [SECRET, NUMBER] {
                assert!(!text.contains(marker), "{label}/{mode} leaked: {text}");
            }
        }
    }
    println!("{} redaction rows, four renderings each", rows.len());
}

#[test]
fn debug_redacts_caller_text_at_every_boundary() {
    let parsed = Document::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"properties":{SECRET:{"type":"string"}}}}}}).to_string()).unwrap();
    let validated = parsed.validate().unwrap().conformant().unwrap();
    let mut rows = Vec::new();
    for diagnostics in [Diagnostics::Coarse, Diagnostics::Upstream] {
        let contracts = validated.value_contracts(
            Arc::new(DefaultEvaluator::new().with_diagnostics(diagnostics)),
            ResourceSet::default(),
        );
        rows.push(row("ValueContracts", &contracts));
        let prepared = contracts.prepare("op", Direction::Input);
        rows.push(row("ValueContractPreparation::Ready", &prepared));
        let ValueContractPreparation::Ready(contract) = prepared else {
            panic!("not ready")
        };
        rows.push(row("ValueContract", &contract));
        let value = JsonValue::parse(serde_json::json!({SECRET:7}).to_string()).unwrap();
        let result = contract.validate(&value);
        assert!(matches!(result, ValueOutcome::Fails { .. }));
        rows.push(row("actual evaluator outcome", &result));
    }

    // Keep all three Upstream probes from the independent review, including their operands.
    let mut rendered = Vec::new();
    for (code, schema, instance) in [
        ("type", serde_json::json!({"type":"string"}), "7"),
        ("required", serde_json::json!({"required":["x"]}), "{}"),
        ("enum", serde_json::json!({"enum":[true]}), "false"),
    ] {
        let document = validate_document(
            serde_json::json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}})
                .to_string(),
        )
        .unwrap()
        .conformant()
        .unwrap();
        let contracts = document.value_contracts(
            Arc::new(DefaultEvaluator::new().with_diagnostics(Diagnostics::Upstream)),
            ResourceSet::default(),
        );
        let ValueContractPreparation::Ready(contract) = contracts.prepare("op", Direction::Input)
        else {
            panic!("not ready")
        };
        let mut result = contract.validate(&JsonValue::parse(instance).unwrap());
        let ValueOutcome::Fails { problems, .. } = &result else {
            panic!("expected failure")
        };
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code, code);
        let item = row_with_fields("Upstream fixed problem code", &result, &[("code", code)]);
        rendered.push(item.text[0].clone());
        rows.push(item);
        let ValueOutcome::Fails { problems, .. } = &mut result else {
            unreachable!()
        };
        problems[0].code = format!("{code}-{SECRET}");
        assert!(serde_json::to_string(&result).unwrap().contains(SECRET));
        rows.push(row_with_fields(
            "Upstream replaced problem code",
            &result,
            &[("code", "<redacted>")],
        ));
    }
    assert_eq!(
        rendered
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );

    check_rows(rows, 14);
}

fn row_with_fields(label: &'static str, value: &impl Debug, fields: &[(&str, &str)]) -> Row {
    let item = row(label, value);
    for (field, expected) in fields {
        let expected = format!("{field}: {expected:?}");
        assert!(
            item.text.iter().all(|text| text.contains(&expected)),
            "{label}: missing {expected}"
        );
    }
    item
}
