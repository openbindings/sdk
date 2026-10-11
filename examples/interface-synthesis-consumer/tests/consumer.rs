use interface_synthesis_consumer::*;
use openbindings::{JsonValue, Source, WorkControl};
use openbindings_interface_synthesis::*;
use std::sync::{Arc, atomic::Ordering};
fn source(text: &str) -> Source {
    Source {
        kind: KIND.into(),
        content: Some(JsonValue::parse(text).unwrap()),
        ..Default::default()
    }
}
#[tokio::test]
async fn standalone_and_shared_provider_consumers() {
    let empty = SynthesisRequest::default();
    assert!(
        Synthesizer::new(Skeleton)
            .synthesize_with_coverage(&empty)
            .await
            .unwrap()
            .coverage()
            .fully_represented()
    );
    let source = source(
        r#"{"units":[{"id":"空.🚲"},{"id":"dep","scope":"dependency"}],"exact":9007199254740993}"#,
    );
    let inputs = [SynthesisSource {
        name: Some("catalog"),
        source: &source,
    }];
    let request = SynthesisRequest {
        sources: &inputs,
        name: Some("My API"),
        ..Default::default()
    };
    let provider = Arc::new(Catalog::default());
    let borrowed = Synthesizer::new(provider.as_ref());
    let shared = Synthesizer::new(provider.clone());
    fn send<T: Send>(_: &T) {}
    let work = borrowed.synthesize_with_coverage(&request);
    send(&work);
    let result = work.await.unwrap();
    let control = WorkControl::new();
    let strict = shared.synthesize_with_control(&request, &control);
    send(&strict);
    drop(control);
    strict.await.unwrap();
    assert_eq!(provider.calls.load(Ordering::Relaxed), 2);
    let inspection = openbindings_source_inspection::Inspector::new(provider.as_ref())
        .inspect(&source)
        .await
        .unwrap();
    assert_eq!(
        inspection.targets[0].source_ref,
        result.coverage().entries()[0].source_ref
    );
    assert_eq!(
        result.coverage().entries()[1].dependency_key.as_deref(),
        Some("s0.u1")
    );
    assert!(
        borrowed
            .check_kind_support([KIND, KIND, "UNKNOWN"])
            .unwrap()[0]
            .supported
    );
    assert_eq!(shared.list_supported_kinds().unwrap().len(), 1);
    drop(source);
    drop(provider);
    assert!(
        result
            .to_json()
            .unwrap()
            .text()
            .contains("9007199254740993")
    );
}
#[tokio::test]
async fn strict_recovery_is_one_call_and_preserves_evidence() {
    for content in [
        r#"{"units":[{"id":"known"},{"id":"gap","status":"implementation-unsupported"}]}"#,
        r#"{"units":[{"id":"known"}],"unknownRemainder":true}"#,
    ] {
        let source = source(content);
        let inputs = [SynthesisSource {
            name: None,
            source: &source,
        }];
        let r = SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        };
        let provider = Catalog::default();
        let e = Synthesizer::new(&provider)
            .synthesize(&r)
            .await
            .unwrap_err();
        assert_eq!(e.kind(), SynthesisErrorKind::Incomplete);
        assert!(e.rejected().is_none());
        let partial = e.into_partial().unwrap();
        assert!(!partial.coverage().fully_represented());
        assert!(!partial.document().parsed().original_bytes().is_empty());
        assert_eq!(provider.calls.load(Ordering::Relaxed), 1);
        let encoded = partial.to_json().unwrap();
        assert!(encoded.get("coverage").is_some());
    }
}
#[tokio::test]
async fn dispositions_and_composition_are_distinct() {
    let p = Catalog::default();
    for status in [
        "represented",
        "invalid",
        "excluded",
        "lossy",
        "implementation-unsupported",
    ] {
        for scope in ["target", "alternative", "projection", "dependency"] {
            let s = source(&format!(
                r#"{{"units":[{{"id":"u","scope":"{scope}","status":"{status}"}}]}}"#
            ));
            let inputs = [SynthesisSource {
                name: None,
                source: &s,
            }];
            let r = SynthesisRequest {
                sources: &inputs,
                ..Default::default()
            };
            let result = Synthesizer::new(&p)
                .synthesize_with_coverage(&r)
                .await
                .unwrap();
            assert_eq!(
                result.coverage().fully_represented(),
                matches!(status, "represented" | "invalid")
            );
            assert_eq!(
                result.coverage().strict_eligible(),
                status != "implementation-unsupported"
            );
        }
    }
    let s = source(r#"{"units":[]}"#);
    let inputs = [
        SynthesisSource {
            name: Some("a"),
            source: &s,
        },
        SynthesisSource {
            name: Some("b"),
            source: &s,
        },
    ];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let doc = Synthesizer::new(&p).synthesize(&r).await.unwrap();
    assert_eq!(doc.parsed().value().get("sources").unwrap().len(), Some(2));
    let one = Catalog {
        single_source: true,
        ..Default::default()
    };
    assert_eq!(
        Synthesizer::new(one)
            .synthesize(&r)
            .await
            .unwrap_err()
            .kind(),
        SynthesisErrorKind::UnsupportedRequest
    );
    let unreadable = source("null");
    let inputs = [
        SynthesisSource {
            name: None,
            source: &s,
        },
        SynthesisSource {
            name: None,
            source: &unreadable,
        },
    ];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    assert_eq!(
        Synthesizer::new(&p)
            .synthesize_with_coverage(&r)
            .await
            .unwrap_err()
            .kind(),
        SynthesisErrorKind::UnreadableSource
    );
}
#[tokio::test]
async fn borrowed_unboxed_future_needs_no_static_provider() {
    use std::future::{Ready, ready};
    struct Local<'a>(&'a std::sync::atomic::AtomicUsize);
    impl InterfaceSynthesizer for Local<'_> {
        type Synthesis<'a>
            = Ready<Result<SynthesisCandidate, SynthesisError>>
        where
            Self: 'a;
        fn supports_kind(&self, _: &str) -> bool {
            false
        }
        fn synthesize<'a>(
            &'a self,
            r: &'a SynthesisRequest<'a>,
            c: WorkControl,
        ) -> Self::Synthesis<'a> {
            self.0.fetch_add(1, Ordering::Relaxed);
            ready(derive(r, &c, false))
        }
    }
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let provider = Local(&calls);
    let facade = Synthesizer::new(&provider);
    let request = SynthesisRequest::default();
    let work = facade.synthesize(&request);
    fn send<T: Send>(_: &T) {}
    send(&work);
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    work.await.unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn fixture_semantics_independently_check_emitted_paths_and_shared_analysis() {
    // This literal catalog is an oracle independent of the facade's report checks.
    let s = source(
        r#"{"units":[{"id":"echo"},{"id":"limited","status":"lossy"},{"id":"retired","status":"excluded"},{"id":"malformed","status":"invalid"},{"id":"gap","status":"implementation-unsupported"}]}"#,
    );
    let provider = Catalog::default();
    let inputs = [SynthesisSource {
        name: None,
        source: &s,
    }];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let result = Synthesizer::new(&provider)
        .synthesize_with_coverage(&r)
        .await
        .unwrap();
    let inspection = openbindings_source_inspection::Inspector::new(&provider)
        .inspect(&s)
        .await
        .unwrap();
    assert_eq!(
        provider.calls.load(Ordering::Relaxed),
        1,
        "inspection must not call synthesis"
    );
    assert_eq!(
        inspection
            .targets
            .iter()
            .map(|t| t.source_ref.as_str())
            .collect::<Vec<_>>(),
        ["echo", "limited", "gap"]
    );
    let doc = result.document().parsed().to_authoring().unwrap();
    let bindings = doc.bindings.unwrap();
    assert_eq!(bindings.len(), 2);
    for binding in bindings.values() {
        let id = binding.content.as_ref().unwrap().view().as_str().unwrap();
        assert!(["echo", "limited"].contains(&id));
        let input = doc.operations[&binding.operation].input.as_ref().unwrap();
        // These are the only schema forms this bounded fixture promises to emit.
        // Both upstream echo targets accept every JSON value; any advertised
        // admitted value must be in that set, and the lossy target narrows to null.
        if id == "echo" {
            assert_eq!(input.view().as_bool(), Some(true));
        } else {
            assert_eq!(input.view().get("type").unwrap().as_str(), Some("null"));
        }
    }
    let unknown = source(r#"{"units":[],"unknownRemainder":true}"#);
    let inventory = openbindings_source_inspection::Inspector::new(&provider)
        .inspect(&unknown)
        .await
        .unwrap();
    assert!(!inventory.exhaustive);
    assert!(inventory.limitation.is_some());
}

#[tokio::test]
async fn malformed_catalog_never_turns_into_empty_or_defaulted_success() {
    for text in [
        "null",
        r#"{"units":[{"id":"u","scope":1}]}"#,
        r#"{"units":[{"id":"u","status":false}]}"#,
        r#"{"units":[],"unknownRemainder":"yes"}"#,
    ] {
        let s = source(text);
        let inputs = [SynthesisSource {
            name: None,
            source: &s,
        }];
        let r = SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        };
        assert_eq!(
            Synthesizer::new(Catalog::default())
                .synthesize_with_coverage(&r)
                .await
                .unwrap_err()
                .kind(),
            SynthesisErrorKind::UnreadableSource
        );
    }
}

#[tokio::test]
async fn persisted_coverage_identifies_sources_after_request_and_provider_drop() {
    async fn synthesize(reverse: bool) -> Synthesis {
        let mut a = source(
            r#"{"units":[{"id":"private","status":"excluded"},{"id":"callback","scope":"dependency"}]}"#,
        );
        a.description = Some("Tenant A".into());
        let mut b = a.clone();
        b.description = Some("Tenant B".into());
        let mut inputs = [
            SynthesisSource {
                name: Some("alpha"),
                source: &a,
            },
            SynthesisSource {
                name: Some("beta"),
                source: &b,
            },
        ];
        if reverse {
            inputs.reverse();
        }
        Synthesizer::new(Catalog::default())
            .synthesize_with_coverage(&SynthesisRequest {
                sources: &inputs,
                ..Default::default()
            })
            .await
            .unwrap()
    }
    let ab = synthesize(false).await;
    let ba = synthesize(true).await;
    assert_eq!(ab.coverage().source_keys(), ["alpha", "beta"]);
    assert_eq!(ba.coverage().source_keys(), ["beta", "alpha"]);
    for result in [ab, ba] {
        let (document, coverage) = result.into_parts();
        // The checked report stays attributable even after splitting the owners.
        assert_eq!(coverage.entries().len(), 4);
        for entry in coverage.entries() {
            assert!(entry.source_key.is_none());
            let key = &coverage.source_keys()[entry.source_index];
            assert!(
                document
                    .parsed()
                    .value()
                    .get("sources")
                    .unwrap()
                    .get(key)
                    .is_some()
            );
        }
    }
    // Repeat with identical excluded-only inventories: before the repair these
    // two complete envelopes were byte-identical despite opposite source indices.
    let mut a = source(r#"{"units":[{"id":"private","status":"excluded"}]}"#);
    a.description = Some("Tenant A".into());
    let mut b = a.clone();
    b.description = Some("Tenant B".into());
    let mut inputs = [
        SynthesisSource {
            name: Some("alpha"),
            source: &a,
        },
        SynthesisSource {
            name: Some("beta"),
            source: &b,
        },
    ];
    let provider = Synthesizer::new(Catalog::default());
    let ab = provider
        .synthesize_with_coverage(&SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        })
        .await
        .unwrap()
        .to_json()
        .unwrap();
    inputs.reverse();
    let ba = provider
        .synthesize_with_coverage(&SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        })
        .await
        .unwrap()
        .to_json()
        .unwrap();
    assert_ne!(ab.text(), ba.text());
    for (envelope, first) in [(&ab, "beta"), (&ba, "alpha")] {
        let reloaded = openbindings::JsonValue::parse(envelope.text()).unwrap();
        let coverage = reloaded.get("coverage").unwrap();
        let keys: Vec<_> = coverage
            .get("sourceKeys")
            .unwrap()
            .elements()
            .unwrap()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(keys[1], first);
    }
}
