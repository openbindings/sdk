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
fn all_55_identity_and_186_directional_groups_preserve_the_published_verdicts() {
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
            let left = fixture.get("left").unwrap().to_owned();
            let right = fixture.get("right").unwrap().to_owned();
            let expected = record["verdict"].as_str().unwrap();
            match document_directional(&left, &right) {
                Ok(found) if verdict(found) == expected => {}
                found => mismatches.push(format!(
                    "{}: {found:?}, expected {expected}",
                    record["path"]
                )),
            }
            assert_eq!(
                expected,
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
            // Profile normalization includes duplicate required names, which
            // core schema admission rejects. Exercise every group directly;
            // also check the public interface fold for conformant carriers.
            if let (Some(required), Some(provided)) =
                (try_conformant(&left), try_conformant(&right))
            {
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
                    summary(&report),
                    document_directional(required.document().value(), provided.document().value()),
                    "{}",
                    record["path"]
                );
            }
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
        (55, 186),
        "every identity and directional group ran"
    );
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    println!(
        "ordinary corpus: {identity} identity and {directional} directional groups passed; 0 skipped"
    );
}
#[test]
fn all_11_identity_and_29_directional_exact_groups_preserve_the_published_verdicts() {
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
        (11, 29),
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

fn try_conformant(value: &JsonValue) -> Option<ConformantDocument> {
    Document::from_json(value.clone())
        .validate()
        .unwrap()
        .conformant()
}
fn document_directional(left: &JsonValue, right: &JsonValue) -> Result<Verdict, ComparisonFailure> {
    let mut outcomes = Vec::new();
    let operations = right.get("operations").unwrap();
    for member in left.get("operations").unwrap().members().unwrap() {
        let name = member.name.as_str().unwrap();
        let identifiers = |value: openbindings::JsonRef<'_>| {
            value
                .get("aliases")
                .into_iter()
                .flat_map(|v| v.elements().unwrap())
                .filter_map(|v| v.as_str())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let mut names = identifiers(member.value);
        names.push(name.to_owned());
        let candidates: Vec<_> = operations
            .members()
            .unwrap()
            .filter(|m| {
                names.iter().any(|name| {
                    m.name.as_str() == Some(name.as_str())
                        || identifiers(m.value).iter().any(|alias| alias == name)
                })
            })
            .collect();
        if candidates.is_empty() {
            outcomes.push(Verdict::Incompatible);
            continue;
        }
        for candidate in candidates {
            for (slot, direction) in [("output", Direction::Output), ("input", Direction::Input)] {
                if let (Some(t), Some(c)) = (member.value.get(slot), candidate.value.get(slot)) {
                    outcomes.push(
                        compare(
                            direction,
                            &SchemaInput::new(t.to_owned(), left.clone()),
                            &SchemaInput::new(c.to_owned(), right.clone()),
                            &ComparisonLimits::default(),
                        )?
                        .verdict(),
                    );
                }
            }
        }
    }
    Ok(Verdict::collapse(outcomes))
}
