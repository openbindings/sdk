mod support;
use openbindings_interface_synthesis::*;
use support::*;

// both named matching and unnamed hash-first matching use exact decimal
// value, including nested values and extension evidence, without binary64.
#[tokio::test]
async fn named_and_unnamed_sources_compare_exact_decimal_values() {
    for (a, b, equal) in [
        ("1", "1.0", true),
        ("1.0", "1e0", true),
        ("-0", "0", true),
        ("-0e999999999999999999999999999", "0.000", true),
        (
            "123456789012345678901234567890.123456789012345678901234567890",
            "123456789012345678901234567890123456789012345678901234567890e-30",
            true,
        ),
        (
            "1e999999999999999999999999999999",
            "10e999999999999999999999999999998",
            true,
        ),
        (
            "1e-999999999999999999999999999999",
            "0.1e-999999999999999999999999999998",
            true,
        ),
        ("1e18446744073709551616", "10e18446744073709551615", true),
        ("1e-18446744073709551616", "0.1e-18446744073709551615", true),
        ("1", "-1", false),
        ("9007199254740993", "9007199254740992", false),
        (
            "1.000000000000000000000000000000000000000000000001",
            "1",
            false,
        ),
        (
            "1e999999999999999999999999999999",
            "1e999999999999999999999999999998",
            false,
        ),
    ] {
        for name in [Some("api"), None] {
            for nested in [false, true] {
                let left = if nested {
                    format!(r#"{{"z":[{a},{{"n":{a}}}],"a":null}}"#)
                } else {
                    a.into()
                };
                let right = if nested {
                    format!(r#"{{"a":null,"z":[{b},{{"n":{b}}}]}}"#)
                } else {
                    b.into()
                };
                let mut input = source(Some(&left));
                let mut output = source(Some(&right));
                input.additional_fields.insert("x-number".into(), json(a));
                output.additional_fields.insert("x-number".into(), json(b));
                let inputs = [SynthesisSource {
                    name,
                    source: &input,
                }];
                let request = SynthesisRequest {
                    sources: &inputs,
                    ..Default::default()
                };
                let result = Synthesizer::new(Fixed::new(candidate(&output)))
                    .synthesize_with_coverage(&request)
                    .await;
                assert_eq!(
                    result.is_ok(),
                    equal,
                    "{a} => {b}, name={name:?}, nested={nested}: {result:?}"
                );
                if let Ok(result) = result {
                    assert_eq!(result.coverage().source_keys(), ["api"]);
                } else {
                    assert_eq!(
                        result
                            .unwrap_err()
                            .rejected()
                            .unwrap()
                            .finding
                            .as_ref()
                            .unwrap()
                            .code,
                        "source-preservation"
                    );
                }
            }
        }
    }
}

// number normalization does not conflate presence or admit ambiguous JSON.
#[tokio::test]
async fn absent_null_and_duplicate_member_policy_remain_distinct() {
    for name in [Some("api"), None] {
        for (left, right) in [(None, Some("null")), (Some("null"), None)] {
            let input = source(left);
            let inputs = [SynthesisSource {
                name,
                source: &input,
            }];
            let request = SynthesisRequest {
                sources: &inputs,
                ..Default::default()
            };
            assert!(
                Synthesizer::new(Fixed::new(candidate(&source(right))))
                    .synthesize_with_coverage(&request)
                    .await
                    .is_err()
            );
        }
        for (left, right) in [
            (r#"{"n":1,"n":2}"#, r#"{"n":1,"n":2}"#),
            (r#"{"n":1,"n":2}"#, r#"{"n":2,"n":1}"#),
            (r#"{"n":1,"n":1.0}"#, r#"{"n":1}"#),
        ] {
            let input = source(Some(left));
            let inputs = [SynthesisSource {
                name,
                source: &input,
            }];
            let request = SynthesisRequest {
                sources: &inputs,
                ..Default::default()
            };
            assert!(
                Synthesizer::new(Fixed::new(candidate(&source(Some(right)))))
                    .synthesize_with_coverage(&request)
                    .await
                    .is_err()
            );
        }
    }
}

// attribution is optional and many-to-one; an emitted dependency does
// not require a keyed evidence row. Supplied keys must still exist.
#[tokio::test]
async fn dependency_attribution_is_optional_and_many_to_one() {
    let input = source(None);
    let mut output = candidate(&input);
    let mut draft = output.document.clone();
    draft.set_operation("event", &Default::default()).unwrap();
    draft
        .set_dependency(
            "consume",
            &openbindings::Dependency {
                operation: "event".into(),
                ..Default::default()
            },
        )
        .unwrap();
    output.document = draft.clone();
    let inputs = [SynthesisSource {
        name: Some("api"),
        source: &input,
    }];
    let request = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let mut row = CoverageEntry::new(
        0,
        "declaration-a",
        CoverageScope::Dependency,
        CoverageStatus::Represented,
    );
    row.dependency_key = Some("consume".into());
    let mut second = row.clone();
    second.source_ref = "declaration-b".into();
    let mut unkeyed = row.clone();
    unkeyed.dependency_key = None;
    unkeyed.source_ref = "declaration-unkeyed".into();
    for rows in [
        vec![],
        vec![unkeyed.clone()],
        vec![row.clone(), second],
        vec![unkeyed, row.clone()],
    ] {
        output.coverage.entries = rows.clone();
        let facade = Synthesizer::new(Fixed::new(output.clone()));
        let result = facade.synthesize_with_coverage(&request).await.unwrap();
        assert_eq!(result.coverage().entries().len(), rows.len());
        for (actual, expected) in result.coverage().entries().iter().zip(&rows) {
            assert_eq!(actual.source_ref, expected.source_ref);
            assert_eq!(actual.dependency_key, expected.dependency_key);
        }
        assert!(facade.synthesize(&request).await.is_ok());
    }
    row.dependency_key = Some("missing".into());
    output.coverage.entries = vec![row];
    let facade = Synthesizer::new(Fixed::new(output));
    for error in [
        facade.synthesize_with_coverage(&request).await.unwrap_err(),
        facade.synthesize(&request).await.unwrap_err(),
    ] {
        assert_eq!(
            error.rejected().unwrap().finding.as_ref().unwrap().code,
            "dependency-correspondence"
        );
    }
}

// independently packaged interfaces retain private helpers. Forwarding and
// byte-budget behavior must agree across borrowed, shared and local handles.
#[tokio::test]
async fn forwarded_synthesizers_preserve_exact_encoding_and_byte_limits() {
    let source = source(Some("9007199254740993"));
    let provider = std::sync::Arc::new(Fixed::new(candidate(&source)));
    let borrowed = Synthesizer::new(&*provider);
    let shared = Synthesizer::new(provider.clone());
    let local = Synthesizer::new(std::rc::Rc::new(provider.clone()));
    let inputs = [SynthesisSource {
        name: Some("api"),
        source: &source,
    }];
    let request = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    for result in [
        borrowed.synthesize_with_coverage(&request).await,
        shared.synthesize_with_coverage(&request).await,
        local.synthesize_with_coverage(&request).await,
    ] {
        let result = result.unwrap();
        let wire = result.to_json().unwrap();
        assert!(
            result
                .to_json_with_limits(openbindings::JsonLimits {
                    max_bytes: wire.bytes().len(),
                    ..Default::default()
                })
                .is_ok()
        );
        assert!(
            result
                .to_json_with_limits(openbindings::JsonLimits {
                    max_bytes: wire.bytes().len() - 1,
                    ..Default::default()
                })
                .is_err()
        );
    }
}

// Two documented SDK policies refuse output the published interface allows:
// repeated identical evidence rows, and an unused input source left out of
// the emitted document.
#[tokio::test]
async fn sdk_policies_refuse_repeated_evidence_and_dropped_unused_sources() {
    let document = openbindings::Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"getUser":{{}}}},"sources":{{"api":{{"kind":"{KIND}"}}}},"bindings":{{"getUser.api":{{"operation":"getUser","source":"api"}}}}}}"#
    ))
    .unwrap();
    let entry = || {
        let mut entry = CoverageEntry::new(
            0,
            "unit",
            CoverageScope::Target,
            CoverageStatus::Represented,
        );
        entry.source_key = Some("api".into());
        entry.operation_key = Some("getUser".into());
        entry.binding_key = Some("getUser.api".into());
        entry
    };
    let used = source(None);
    let unused = openbindings::Source {
        description: Some("contributes no target".into()),
        ..source(None)
    };
    let finding = |result: Result<Synthesis, SynthesisError>| {
        let finding = result
            .unwrap_err()
            .rejected()
            .unwrap()
            .finding
            .clone()
            .unwrap();
        (finding.code, finding.path)
    };
    let one = [SynthesisSource {
        name: Some("api"),
        source: &used,
    }];
    let repeated = SynthesisCandidate::new(
        document.clone(),
        CoverageClaim::complete(vec![entry(), entry()]),
    );
    let result = Synthesizer::new(Fixed::new(repeated))
        .synthesize_with_coverage(&SynthesisRequest {
            sources: &one,
            ..Default::default()
        })
        .await;
    assert_eq!(finding(result).0, "duplicate-unit");
    let two = [
        SynthesisSource {
            name: Some("api"),
            source: &used,
        },
        SynthesisSource {
            name: None,
            source: &unused,
        },
    ];
    let dropped = SynthesisCandidate::new(document, CoverageClaim::complete(vec![entry()]));
    let result = Synthesizer::new(Fixed::new(dropped))
        .synthesize_with_coverage(&SynthesisRequest {
            sources: &two,
            ..Default::default()
        })
        .await;
    assert_eq!(
        finding(result),
        (
            "source-preservation".to_owned(),
            "/request/sources/1".to_owned()
        )
    );
}
