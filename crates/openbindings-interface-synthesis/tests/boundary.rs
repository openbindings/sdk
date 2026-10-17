mod support;
use openbindings::{Document, JsonLimits, VersionRefusal};
use openbindings_interface_synthesis::CancellationToken;
use openbindings_interface_synthesis::*;
use std::{
    error::Error,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
use support::*;

#[tokio::test]
async fn complete_claim_still_requires_facade_correspondence_checks() {
    // CA-3: complete claims an exhaustive inventory without bypassing validation.
    let s = source(None);
    let mut c = candidate(&s);
    c.coverage = CoverageClaim::complete(Vec::new());
    assert!(c.coverage.exhaustive);
    assert!(c.coverage.limitation.is_none());
    assert!(c.coverage.additional_fields.is_empty());
    let checked = deliver(&s, c.clone()).await.unwrap();
    assert!(checked.coverage().exhaustive());
    assert!(checked.coverage().entries().is_empty());
    assert!(checked.coverage().limitation().is_none());

    c.coverage = CoverageClaim::complete(vec![CoverageEntry::new(
        0,
        "missing-path",
        CoverageScope::Target,
        CoverageStatus::Represented,
    )]);
    assert_eq!(c.coverage.entries.len(), 1);
    assert_eq!(c.coverage.entries[0].source_ref, "missing-path");
    let error = deliver(&s, c).await.unwrap_err();
    assert_eq!(error.kind(), SynthesisErrorKind::InvalidResult);
}

#[tokio::test]
async fn admission_is_lazy_and_precedes_provider_work() {
    let s = source(None);
    let c = candidate(&s);
    let p = Fixed::new(c);
    let f = Synthesizer::new(&p);
    for version in ["0.3.0", "0.2.0-rc.1", "bad"] {
        let r = SynthesisRequest {
            openbindings_version: Some(version),
            ..Default::default()
        };
        let work = f.synthesize(&r);
        assert_eq!(p.calls.load(Ordering::Relaxed), 0);
        let e = work.await.unwrap_err();
        assert_eq!(e.kind(), SynthesisErrorKind::InvalidInput);
        if version == "bad" {
            assert!(e.code().ends_with("malformed-version"));
        } else {
            assert_eq!(
                e.source()
                    .unwrap()
                    .downcast_ref::<VersionRefusal>()
                    .unwrap()
                    .declared,
                version
            );
        }
    }
    for name in ["", "bad name", "空", ".leading"] {
        let inputs = [SynthesisSource {
            name: Some(name),
            source: &s,
        }];
        let r = SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        };
        let e = f.synthesize(&r).await.unwrap_err();
        assert!(e.code().ends_with("invalid-source-name"));
    }
    let mut unsupported = s.clone();
    unsupported.kind = "other".into();
    let mut illegal = s.clone();
    illegal
        .additional_fields
        .insert("legacy".into(), json("true"));
    let mut empty = s.clone();
    empty.kind.clear();
    for bad in [&unsupported, &illegal, &empty] {
        let inputs = [
            SynthesisSource {
                name: None,
                source: &s,
            },
            SynthesisSource {
                name: None,
                source: bad,
            },
        ];
        let r = SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        };
        assert!(f.synthesize(&r).await.is_err());
    }
    let null = source(Some("null"));
    let inputs = [
        SynthesisSource {
            name: Some("same"),
            source: &s,
        },
        SynthesisSource {
            name: Some("same"),
            source: &null,
        },
    ];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    assert_eq!(
        f.synthesize(&r).await.unwrap_err().kind(),
        SynthesisErrorKind::InvalidInput
    );
    let control = CancellationToken::new();
    control.cancel();
    assert_eq!(
        f.synthesize_with_control(&SynthesisRequest::default(), &control)
            .await
            .unwrap_err()
            .kind(),
        SynthesisErrorKind::Cancelled
    );
    assert_eq!(p.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn provider_cannot_forge_strict_recovery() {
    let (s, mut c) = complete();
    c.coverage.entries[0].status = CoverageStatus::ImplementationUnsupported;
    c.coverage.entries[0].reason_code = Some("gap".into());
    c.coverage.entries[0].message = Some("gap".into());
    let inputs = [SynthesisSource {
        name: Some("api"),
        source: &s,
    }];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let real = Synthesizer::new(Fixed::new(c))
        .synthesize(&r)
        .await
        .unwrap_err();
    assert!(real.partial().is_some());
    for forged in [
        SynthesisError::new(SynthesisErrorKind::Incomplete, "lie", "no partial"),
        real,
    ] {
        let p = Fixed {
            output: Err(forged),
            calls: AtomicUsize::new(0),
        };
        let e = Synthesizer::new(p)
            .synthesize(&SynthesisRequest::default())
            .await
            .unwrap_err();
        assert_eq!(e.kind(), SynthesisErrorKind::ProviderFailure);
        assert!(e.partial().is_none());
        assert!(e.source().is_some());
    }
}

#[tokio::test]
async fn source_preservation_is_exact_structural_and_panic_free() {
    for (a, b, equal) in [
        (
            r#"{"a":9007199254740993,"b":[null,true]}"#,
            r#"{ "b": [null,true], "a": 9007199254740993 }"#,
            true,
        ),
        // Equivalent surrogate-pair spellings preserve opaque source content.
        (r#""\uD800\uDC00""#, r#""\ud800\udc00""#, true),
        (r#""\uD800\uDC00""#, r#""\"\\uD800\\uDC00\"""#, false),
        // opaque member names also compare as exact UTF-16 units.
        (
            r#"{"\uD800\uDC00":1,"a":2}"#,
            r#"{"a":2,"\ud800\udc00":1}"#,
            true,
        ),
        ("9007199254740993", "9007199254740992", false),
        ("1.0", "1", true),
        (r#"{"x":1,"x":2}"#, r#"{"x":2,"x":1}"#, false),
    ] {
        let s = source(Some(a));
        let other = source(Some(b));
        let outcome = deliver(&s, candidate(&other)).await;
        assert_eq!(outcome.is_ok(), equal, "{a} => {b}: {outcome:?}");
    }
    let absent = source(None);
    let null = source(Some("null"));
    assert!(deliver(&absent, candidate(&null)).await.is_err());
    let s = source(None);
    let mut c = candidate(&s);
    c.document=Document::parse(r#"{"openbindings":"0.2.0","operations":{},"sources":{"\ud800\udc00":{"kind":"example.coverage@1"}}}"#).unwrap();
    assert!(deliver(&s, c).await.is_err());
}

#[tokio::test]
async fn source_indices_names_and_projection_identity() {
    let (s, c) = complete();
    let inputs = [
        SynthesisSource {
            name: Some("api"),
            source: &s,
        },
        SynthesisSource {
            name: Some("api"),
            source: &s,
        },
    ];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let mut repeated = c.clone();
    let mut second = repeated.coverage.entries[0].clone();
    second.source_index = 1;
    repeated.coverage.entries.push(second);
    let result = Synthesizer::new(Fixed::new(repeated))
        .synthesize_with_coverage(&r)
        .await
        .unwrap();
    assert_eq!(result.coverage().entries().len(), 2);
    assert_eq!(result.coverage().source_keys(), ["api", "api"]);
    assert_eq!(
        result
            .document()
            .as_document()
            .value()
            .get("sources")
            .unwrap()
            .len(),
        Some(1)
    );
    let unnamed = [
        SynthesisSource {
            name: None,
            source: &s,
        },
        SynthesisSource {
            name: None,
            source: &s,
        },
    ];
    let r = SynthesisRequest {
        sources: &unnamed,
        ..Default::default()
    };
    let mut shared = c.clone();
    let mut second = shared.coverage.entries[0].clone();
    second.source_index = 1;
    shared.coverage.entries.push(second);
    assert!(
        Synthesizer::new(Fixed::new(shared))
            .synthesize_with_coverage(&r)
            .await
            .is_ok()
    );
    let mut reused = c.clone();
    let binding = reused
        .document
        .binding("getUser.api")
        .unwrap()
        .unwrap()
        .value()
        .to_owned();
    reused.document.set("/bindings/second", &binding).unwrap();
    let e = &mut reused.coverage.entries[0];
    e.scope = CoverageScope::Projection;
    let mut second = e.clone();
    second.binding_key = Some("second".into());
    reused.coverage.entries.push(second);
    assert!(deliver(&s, reused.clone()).await.is_ok());
    reused
        .coverage
        .entries
        .push(reused.coverage.entries[1].clone());
    assert!(deliver(&s, reused).await.is_err());
    let mut dropped = c.clone();
    let b = Document::new();
    dropped.document = b.clone();
    dropped.coverage.entries.clear();
    assert!(deliver(&s, dropped).await.is_err());
}

#[tokio::test]
async fn every_boundary_failure_retains_a_repairable_candidate() {
    let (s, c) = complete();
    for mutation in 0..15 {
        let mut bad = c.clone();
        match mutation {
            0 => bad.coverage.entries[0].source_index = 4,
            1 => bad.coverage.entries[0].binding_key = Some("missing".into()),
            2 => bad.coverage.entries[0].operation_key = Some("missing".into()),
            3 => bad.coverage.entries[0].source_key = Some("missing".into()),
            4 => bad.coverage.entries.push(bad.coverage.entries[0].clone()),
            5 => bad.coverage.exhaustive = false,
            6 => {
                bad.coverage.exhaustive = false;
                bad.coverage.limitation = Some(CoverageLimitation::new("", "unknown"));
            }
            7 => bad
                .coverage
                .additional_fields
                .insert("fullyRepresented".into(), json("true"))
                .map(|_| ())
                .unwrap_or(()),
            8 => {
                let e = &mut bad.coverage.entries[0];
                e.status = CoverageStatus::Invalid;
                e.reason_code = Some("invalid".into());
                e.message = Some("".into());
                e.source_key = Some("".into());
            }
            9 => bad.coverage.entries[0].requirements = Some(vec!["".into()]),
            10 => {
                bad.coverage.entries[0].status = CoverageStatus::Excluded;
                bad.coverage.entries[0].reason_code = Some("exclude".into());
                bad.coverage.entries[0].message = Some("why".into());
            }
            11 => bad.coverage.entries[0].dependency_key = Some("wrong-scope".into()),
            12 => {
                bad.document =
                    Document::parse(r#"{"openbindings":"0.2.0","operations":null}"#).unwrap()
            }
            13 => {
                bad.document =
                    Document::parse(r#"{"openbindings":"1.0.0","operations":{}}"#).unwrap()
            }
            14 => {
                bad.coverage
                    .additional_fields
                    .insert("sourceKeys".into(), json(r#"["forged"]"#));
            }
            _ => unreachable!(),
        }
        let e = deliver(&s, bad).await.unwrap_err();
        assert_eq!(e.kind(), SynthesisErrorKind::InvalidResult, "{mutation}");
        assert!(e.partial().is_none());
        let rejected = e.into_rejected().unwrap();
        assert!(rejected.validation.is_some() || mutation == 13);
        if mutation < 12 || mutation == 14 {
            assert!(rejected.finding.is_some());
        }
        if mutation == 14 {
            let finding = rejected.finding.as_ref().unwrap();
            assert_eq!(finding.code, "shadowed-field");
            assert_eq!(finding.path, "/coverage/sourceKeys");
        }
        let _editable = rejected.candidate.document.clone();
    }
    // A real wrong-input source link: both carriers exist, so simple key membership is insufficient.
    let different = source(Some("null"));
    let inputs = [
        SynthesisSource {
            name: Some("api"),
            source: &s,
        },
        SynthesisSource {
            name: Some("other"),
            source: &different,
        },
    ];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let mut c = c;
    c.document.set_source("other", &different).unwrap();
    c.coverage.entries[0].source_index = 1;
    assert!(
        Synthesizer::new(Fixed::new(c))
            .synthesize_with_coverage(&r)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn exact_persistence_budgets_and_editing_do_not_reassign_evidence() {
    let mut s = source(Some(r#"{"unicode":"空🚲","large":9007199254740993}"#));
    s.additional_fields
        .insert("x-exact".into(), json("9007199254740993"));
    let mut c = candidate(&s);
    c.coverage
        .additional_fields
        .insert("opaque".into(), json(r#"{"a":1,"a":9007199254740993}"#));
    let result = deliver(&s, c).await.unwrap();
    drop(s);
    assert_eq!(result.coverage().source_keys(), ["api"]);
    let encoded = result.to_json().unwrap();
    assert!(encoded.text().contains("9007199254740993"));
    assert_eq!(encoded.text(), serde_json::to_string(&result).unwrap());
    let bytes = encoded.text().len();
    assert!(
        result
            .to_json_with_limits(JsonLimits {
                max_bytes: bytes,
                ..Default::default()
            })
            .is_ok()
    );
    for limits in [
        JsonLimits {
            max_bytes: bytes - 1,
            ..Default::default()
        },
        JsonLimits {
            max_nodes: 2,
            ..Default::default()
        },
        JsonLimits {
            max_depth: 1,
            ..Default::default()
        },
    ] {
        assert_eq!(
            result.to_json_with_limits(limits).unwrap_err().kind(),
            SynthesisErrorKind::Encoding
        );
    }
    let reloaded = json(encoded.text());
    let doc = Document::parse(reloaded.get("interface").unwrap().text()).unwrap();
    assert_eq!(
        reloaded
            .get("coverage")
            .unwrap()
            .get("opaque")
            .unwrap()
            .get("a")
            .unwrap()
            .number_text(),
        Some("1")
    );
    let mut edited = doc.clone();
    edited.set("/sources/api/content", &json("null")).unwrap();
    assert_ne!(edited.as_bytes(), doc.as_bytes());
    assert!(
        result
            .to_json()
            .unwrap()
            .text()
            .contains("9007199254740993")
    );
}

// the publication permits an unbound contract even in a partial result.
#[tokio::test]
async fn unbound_contracts_pass_strict_and_partial_synthesis() {
    let s = source(None);
    let mut c = candidate(&s);
    let mut draft = c.document.clone();
    draft.set_operation("event", &Default::default()).unwrap();
    c.document = draft.clone();
    let inputs = [SynthesisSource {
        name: Some("api"),
        source: &s,
    }];
    let request = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let facade = Synthesizer::new(Fixed::new(c.clone()));
    assert!(facade.synthesize_with_coverage(&request).await.is_ok());
    assert!(facade.synthesize(&request).await.is_ok());
    let mut unsupported = CoverageEntry::new(
        0,
        "admitted-event",
        CoverageScope::Target,
        CoverageStatus::ImplementationUnsupported,
    );
    unsupported.reason_code = Some("example.unsupported".into());
    unsupported.message = Some("An admitted target cannot be framed".into());
    c.coverage.entries.push(unsupported);
    let facade = Synthesizer::new(Fixed::new(c));
    assert!(facade.synthesize_with_coverage(&request).await.is_ok());
    let error = facade.synthesize(&request).await.unwrap_err();
    assert_eq!(error.kind(), SynthesisErrorKind::Incomplete);
    assert!(
        error
            .partial()
            .unwrap()
            .document()
            .as_document()
            .value()
            .get("operations")
            .unwrap()
            .get("event")
            .is_some()
    );
}

#[tokio::test]
async fn unnamed_source_mapping_is_stable_without_restricting_equal_binding_aliases() {
    let (s, mut c) = complete();
    // Binding uses "api"; the mapping may use an equal alias.
    c.document.set_source("alias", &s).unwrap();
    let inputs = [SynthesisSource {
        name: None,
        source: &s,
    }];
    let request = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    let result = Synthesizer::new(Fixed::new(c))
        .synthesize_with_coverage(&request)
        .await
        .unwrap();
    assert_eq!(result.coverage().source_keys(), ["alias"]);
    assert_eq!(
        result.coverage().entries()[0].source_key.as_deref(),
        Some("api")
    );
}

struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
struct PendingProvider {
    started: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}
struct PendingWork(Arc<AtomicUsize>);
impl Future for PendingWork {
    type Output = Result<SynthesisCandidate, SynthesisError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for PendingWork {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
impl InterfaceSynthesizer for PendingProvider {
    type Synthesis<'a> = PendingWork;
    fn supports_kind(&self, _: &str) -> bool {
        true
    }
    fn synthesize<'a>(&'a self, _: &'a SynthesisRequest<'a>, _: CancellationToken) -> PendingWork {
        self.started.fetch_add(1, Ordering::Relaxed);
        PendingWork(self.dropped.clone())
    }
}
#[test]
fn cancellation_wakes_and_drops_both_pending_calls() {
    let started = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let facade = Synthesizer::new(PendingProvider {
        started: started.clone(),
        dropped: dropped.clone(),
    });
    let request = SynthesisRequest::default();
    let control = CancellationToken::new();
    let unpolled = facade.synthesize(&request);
    drop(unpolled);
    assert_eq!(started.load(Ordering::Relaxed), 0);
    let mut a = Box::pin(facade.synthesize_with_control(&request, &control));
    let mut b = Box::pin(facade.synthesize_with_coverage_and_control(&request, &control));
    let wa = Arc::new(WakeCount(AtomicUsize::new(0)));
    let wb = Arc::new(WakeCount(AtomicUsize::new(0)));
    let wakea = Waker::from(wa.clone());
    let wakeb = Waker::from(wb.clone());
    let mut ca = Context::from_waker(&wakea);
    let mut cb = Context::from_waker(&wakeb);
    assert!(a.as_mut().poll(&mut ca).is_pending());
    assert!(b.as_mut().poll(&mut cb).is_pending());
    assert_eq!(started.load(Ordering::Relaxed), 2);
    control.cancel();
    assert!(wa.0.load(Ordering::Relaxed) > 0);
    assert!(wb.0.load(Ordering::Relaxed) > 0);
    assert!(
        matches!(a.as_mut().poll(&mut ca),Poll::Ready(Err(e)) if e.kind()==SynthesisErrorKind::Cancelled)
    );
    assert!(
        matches!(b.as_mut().poll(&mut cb),Poll::Ready(Err(e)) if e.kind()==SynthesisErrorKind::Cancelled)
    );
    assert_eq!(dropped.load(Ordering::Relaxed), 2);
    let mut abandoned = Box::pin(facade.synthesize(&request));
    assert!(abandoned.as_mut().poll(&mut ca).is_pending());
    drop(abandoned);
    assert_eq!(dropped.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn unpaired_source_content_cannot_be_published_as_a_conformant_document() {
    let s = source(Some(r#""\ud800""#));
    let error = deliver(&s, candidate(&s)).await.unwrap_err();
    assert_eq!(error.kind(), SynthesisErrorKind::InvalidResult);
    let rejected = error.rejected().unwrap();
    let validation = rejected.validation.as_ref().unwrap();
    assert_eq!(
        validation.report().conclusion,
        openbindings::Conformance::NonConformant
    );
    assert_eq!(
        validation.report().evidence["OBI-01"],
        openbindings::Evidence::Violated
    );
    assert_eq!(
        validation.report().evidence["OBI-02"],
        openbindings::Evidence::NotApplicable
    );
    let finding = &validation.report().findings[0];
    assert_eq!(finding.rule, "OBI-01");
    assert_eq!(
        finding
            .location
            .as_ref()
            .unwrap()
            .pointer
            .as_ref()
            .and_then(openbindings::Pointer::as_str),
        Some("/sources/api/content")
    );
    assert!(
        rejected
            .candidate
            .document
            .value()
            .text()
            .contains("\\ud800")
    );
}

#[test]
fn provider_constructors_fill_the_fields_they_name() {
    let kind = SupportedKind::new(KIND).with_description("echo values");
    assert_eq!(
        (kind.kind.as_str(), kind.description.as_deref()),
        (KIND, Some("echo values"))
    );
    let evidence = std::collections::BTreeMap::from([(
        "count".to_owned(),
        openbindings::JsonValue::parse("2").unwrap(),
    )]);
    let limitation = CoverageLimitation::new("example.remaining", "two units were not read")
        .with_details(evidence.clone())
        .with_additional_fields(evidence);
    assert_eq!(
        (limitation.code.as_str(), limitation.message.as_str()),
        ("example.remaining", "two units were not read")
    );
    assert!(limitation.details.is_some() && limitation.additional_fields.len() == 1);
    let entry = CoverageEntry::new(0, "echo", CoverageScope::Target, CoverageStatus::Lossy);
    let partial = CoverageClaim::partial(vec![entry.clone()], limitation);
    assert!(!partial.exhaustive && partial.limitation.is_some());
    assert_eq!(partial.entries.len(), 1);
    let complete = CoverageClaim::complete(vec![entry]);
    assert!(complete.exhaustive && complete.limitation.is_none());
    let candidate = SynthesisCandidate::new(Document::new(), complete);
    assert!(candidate.coverage.exhaustive);
}
