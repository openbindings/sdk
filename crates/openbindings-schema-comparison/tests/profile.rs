use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_schema_comparison::*;
use std::{collections::BTreeSet, path::PathBuf};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn document_identity(left: &JsonValue, right: &JsonValue) -> Result<bool, ComparisonFailure> {
    let left_operations = left.get("operations").unwrap();
    let right_operations = right.get("operations").unwrap();
    let left_keys: BTreeSet<_> = left_operations
        .members()
        .unwrap()
        .map(|m| m.name.as_str().unwrap())
        .collect();
    let right_keys: BTreeSet<_> = right_operations
        .members()
        .unwrap()
        .map(|m| m.name.as_str().unwrap())
        .collect();
    if left_keys != right_keys {
        return Ok(false);
    }
    let mut answer = true;
    for key in left_keys {
        for slot in ["input", "output"] {
            let a = left_operations
                .get(key)
                .unwrap()
                .get(slot)
                .map(|schema| SchemaInput::new(schema.to_owned(), left.clone()));
            let b = right_operations
                .get(key)
                .unwrap()
                .get(slot)
                .map(|schema| SchemaInput::new(schema.to_owned(), right.clone()));
            if !identical(a.as_ref(), b.as_ref(), &ComparisonLimits::default())? {
                answer = false;
            }
        }
    }
    Ok(answer)
}
#[test]
fn all_30_identity_and_62_directional_groups_preserve_the_published_verdicts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/manifest.json")).unwrap();
    let mut identity = 0;
    let mut directional = 0;
    let mut mismatches = Vec::new();
    for record in manifest["files"].as_array().unwrap() {
        if record["mode"] != "identical" {
            directional += 1;
            let fixture = json(
                &std::fs::read_to_string(root.join(record["path"].as_str().unwrap())).unwrap(),
            );
            let required = conformant(&fixture.get("left").unwrap().to_owned());
            let provided = conformant(&fixture.get("right").unwrap().to_owned());
            let report = check_interface(
                &required,
                &provided,
                &ComparisonContext::new(required.document().value().clone()),
                &ComparisonContext::new(provided.document().value().clone()),
                &|_, _| Some(OperationRole::Realized),
                &ComparisonLimits::default(),
            )
            .unwrap();
            assert_eq!(
                verdict(summary(&report).unwrap()),
                record["verdict"].as_str().unwrap(),
                "{}",
                record["path"]
            );
            assert_eq!(
                verdict(summary(&report).unwrap()),
                fixture
                    .get("expected")
                    .unwrap()
                    .get("summary")
                    .unwrap()
                    .get("verdict")
                    .unwrap()
                    .as_str()
                    .unwrap()
            );
            let actual: BTreeSet<_> = report
                .issues()
                .map(|issue| {
                    if issue.kind() == InterfaceIssueKind::Missing {
                        return "operation-removed";
                    }
                    if matches!(
                        issue.kind(),
                        InterfaceIssueKind::InputIndeterminate
                            | InterfaceIssueKind::OutputIndeterminate
                    ) {
                        return "outside-profile";
                    }
                    let reason = issue.reason().unwrap();
                    if reason.ends_with(": candidate property not allowed by target") {
                        "additionalProperties"
                    } else if reason.starts_with("properties[")
                        && ["minimum:", "maximum:"]
                            .iter()
                            .any(|key| reason.contains(key))
                    {
                        "numeric-bounds"
                    } else if reason.starts_with("properties[") {
                        "properties"
                    } else if reason.starts_with("items:") {
                        "items"
                    } else if reason.starts_with("minimum:") || reason.starts_with("maximum:") {
                        "numeric-bounds"
                    } else if ["minLength:", "maxLength:", "minItems:", "maxItems:"]
                        .iter()
                        .any(|prefix| reason.starts_with(prefix))
                    {
                        "length-bounds"
                    } else if reason.starts_with("oneOf:") || reason.starts_with("anyOf:") {
                        "union"
                    } else {
                        reason.split(':').next().unwrap()
                    }
                })
                .collect();
            let expected: BTreeSet<_> = record["findings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(actual, expected, "{} findings", record["path"]);
            continue;
        }
        identity += 1;
        let fixture =
            json(&std::fs::read_to_string(root.join(record["path"].as_str().unwrap())).unwrap());
        let result = document_identity(
            &fixture.get("left").unwrap().to_owned(),
            &fixture.get("right").unwrap().to_owned(),
        );
        let expected = record["verdict"] == "compatible";
        if result != Ok(expected) {
            mismatches.push(format!(
                "{}: {result:?}, expected {expected:?}",
                record["path"]
            ));
        }
    }
    assert_eq!(
        (identity, directional),
        (30, 62),
        "every identity and directional group ran"
    );
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    println!(
        "ordinary corpus: {identity} identity and {directional} directional groups passed; 0 skipped"
    );
}
#[test]
fn all_10_identity_and_29_directional_exact_groups_preserve_the_published_verdicts() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/exact-values.json")).unwrap();
    let mut identity = 0;
    let mut directional = 0;
    for case in pack["cases"].as_array().unwrap() {
        if case["mode"] != "identical" {
            directional += 1;
            let required = conformant(&json(case["leftJSON"].as_str().unwrap()));
            let provided = conformant(&json(case["rightJSON"].as_str().unwrap()));
            let report = check_interface(
                &required,
                &provided,
                &ComparisonContext::new(required.document().value().clone()),
                &ComparisonContext::new(provided.document().value().clone()),
                &|_, _| Some(OperationRole::Realized),
                &ComparisonLimits::default(),
            )
            .unwrap();
            if case["expected"]["error"] == "schema" {
                assert_eq!(
                    (summary(&report)).map_err(|failure| failure.kind()),
                    Err(ComparisonFailureKind::UnsatisfiableMerge),
                    "{}",
                    case["id"]
                );
            } else {
                assert_eq!(
                    verdict(summary(&report).unwrap()),
                    case["expected"]["verdict"].as_str().unwrap(),
                    "{}",
                    case["id"]
                );
            }
            if let Some(reason) = case["reason"].as_str() {
                assert_eq!(
                    report.issues().next().unwrap().reason(),
                    Some(reason),
                    "{}",
                    case["id"]
                );
            }
            continue;
        }
        identity += 1;
        let result = document_identity(
            &json(case["leftJSON"].as_str().unwrap()),
            &json(case["rightJSON"].as_str().unwrap()),
        );
        let expected = case["expected"]["verdict"] == "compatible";
        assert_eq!(result, Ok(expected), "{}", case["id"]);
    }
    assert_eq!(
        (identity, directional),
        (10, 29),
        "every identity and directional group ran"
    );
    println!(
        "exact corpus: {identity} identity and {directional} directional groups passed; 0 skipped (including expected normalization failures)"
    );
}

fn conformant(value: &JsonValue) -> ConformantDocument {
    Document::from_json(value.clone())
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant corpus document")
}
fn verdict(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Compatible => "compatible",
        Verdict::Incompatible => "incompatible",
        Verdict::Indeterminate => "indeterminate",
    }
}

fn summary(report: &InterfaceComparison) -> Result<Verdict, ComparisonFailure> {
    let mut verdicts = Vec::new();
    for operation in report.operations() {
        if operation.is_missing() {
            verdicts.push(Verdict::Incompatible);
        }
        for slot in [operation.output(), operation.input()]
            .into_iter()
            .flatten()
        {
            verdicts.push(slot.as_ref().map_err(Clone::clone)?.verdict());
        }
    }
    Ok(Verdict::collapse(verdicts))
}
