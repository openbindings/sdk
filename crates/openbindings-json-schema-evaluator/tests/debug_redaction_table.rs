//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::fmt::Debug;
// A credential-shaped probe value; Debug output must never contain it.
const SECRET: &str = "PROBE-BEARER-TOKEN-7391";
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
    let operation = validated.resolve_operation("op").unwrap();
    let mut rows = Vec::new();
    for detail in [Detail::Outcome, Detail::Explain] {
        let unprepared = operation.input().unwrap();
        rows.push(row("ValueContract", &unprepared));
        let prepared = unprepared.prepare(&DefaultEvaluator::new());
        rows.push(row("Ok(PreparedContract)", &prepared));
        let Ok(contract) = prepared else {
            panic!("not ready")
        };
        rows.push(row("PreparedContract", &contract));
        let value = JsonValue::parse(serde_json::json!({SECRET:7}).to_string()).unwrap();
        let result = match detail {
            Detail::Outcome => contract.validate(&value),
            _ => contract.explain(&value),
        };
        assert!(matches!(result, ValueOutcome::Fails { .. }));
        rows.push(row("actual evaluator outcome", &result));
    }

    // Three explanation probes, including their operands.
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
        let operation = document.resolve_operation("op").unwrap();
        let Ok(contract) = operation.input().unwrap().prepare(&DefaultEvaluator::new()) else {
            panic!("not ready")
        };
        let mut result = contract.explain(&JsonValue::parse(instance).unwrap());
        let ValueOutcome::Fails { problems, .. } = &result else {
            panic!("expected failure")
        };
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code, code);
        let item = row_with_fields("Explained fixed problem code", &result, &[("code", code)]);
        rendered.push(item.text[0].clone());
        rows.push(item);
        let ValueOutcome::Fails { problems, .. } = &mut result else {
            unreachable!()
        };
        problems[0].code = format!("{code}-{SECRET}");
        assert!(serde_json::to_string(&result).unwrap().contains(SECRET));
        rows.push(row_with_fields(
            "Explained replaced problem code",
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
