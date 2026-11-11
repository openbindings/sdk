use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::Value;

#[test]
fn supplied_resources_keep_their_own_reference_context_and_origins() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/supplied-roots.json")).unwrap();
    for case in cases {
        let uri = case["uri"].as_str().unwrap();
        let mut entries = vec![ExternalSchema {
            uri: uri.into(),
            document: JsonValue::parse(case["schema"].to_string()).unwrap(),
        }];
        for (uri, schema) in case["resources"].as_object().into_iter().flatten() {
            entries.push(ExternalSchema {
                uri: uri.clone(),
                document: JsonValue::parse(schema.to_string()).unwrap(),
            });
        }
        let resources = ExternalSchemas::new(entries).unwrap();
        let prepared = resources.prepare(uri, &DefaultEvaluator::new()).unwrap();
        for row in case["values"].as_array().unwrap() {
            let value = JsonValue::parse(row["value"].to_string()).unwrap();
            for outcome in [prepared.evaluate(&value), prepared.explain(&value)] {
                let status = match &outcome {
                    ValueOutcome::Satisfies => "satisfies",
                    ValueOutcome::Fails { problems, .. } => {
                        for problem in problems {
                            assert_eq!(
                                problem.schema.source_uri.as_deref(),
                                Some(row["source_uri"].as_str().unwrap_or(uri))
                            );
                            assert_eq!(problem.schema.pointer.as_str(), row["pointer"].as_str());
                            assert!(resources.locate(&problem.schema).is_some());
                        }
                        "fails"
                    }
                    ValueOutcome::Undefined { .. } => "undefined",
                    ValueOutcome::NoVerdict { .. } => "no-verdict",
                };
                assert_eq!(status, row["status"].as_str().unwrap(), "{}", case["name"]);
            }
            if row["status"] == "fails" {
                assert!(
                    matches!(prepared.explain(&value), ValueOutcome::Fails { problems, problems_complete: true, .. } if !problems.is_empty())
                );
            }
        }
        // The prepared owner retains the original resource context.
        let row = &case["values"][0];
        let value = JsonValue::parse(row["value"].to_string()).unwrap();
        let before = prepared.evaluate(&value);
        drop(resources);
        assert_eq!(prepared.evaluate(&value), before);
    }
}

#[test]
fn supplied_root_refusals_are_preparation_failures_with_safe_diagnostics() {
    let uri = "https://private.example/root";
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: uri.into(),
        document: JsonValue::parse(r#"{"$id":"https://private.example/declared","type":5}"#)
            .unwrap(),
    }])
    .unwrap();
    let evaluator = DefaultEvaluator::new();
    for missing in [
        "https://private.example/missing",
        "https://private.example/declared",
    ] {
        let error = resources.prepare(missing, &evaluator).unwrap_err();
        assert_eq!(
            error.kind,
            PreparationFailureKind::Resolution {
                cause: ResolutionFailureKind::ResourceUnavailable {
                    uri: missing.into()
                },
            }
        );
        assert!(!format!("{error} {error:?}").contains("private.example"));
    }
    let error = resources.prepare(uri, &evaluator).unwrap_err();
    assert_eq!(error.kind, PreparationFailureKind::InvalidExternalSchema);
    assert_eq!(error.origin.unwrap().source_uri.as_deref(), Some(uri));
}
