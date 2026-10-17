use openbindings::{JsonLimits, JsonValue, Operation, Source};
use openbindings_source_inspection::CancellationToken;
use openbindings_source_inspection::*;
use std::{
    cell::Cell,
    collections::BTreeMap,
    future::{Future, Ready, ready},
    pin::{Pin, pin},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

const KIND: &str = "urn:example:echo";
fn source() -> Source {
    Source {
        kind: KIND.into(),
        ..Default::default()
    }
}
fn target() -> BindableTarget {
    BindableTarget::new("one", BindingFragment::default())
}
// A provider can still assign the public flag and return an incomplete
// inventory without evidence; the facade refuses it.
fn incomplete_without_evidence() -> SourceInspection {
    let mut inventory = SourceInspection::complete(Vec::new());
    inventory.exhaustive = false;
    inventory
}
fn without_source_ref() -> BindableTarget {
    let mut item = target();
    item.source_ref = String::new();
    item
}

struct Echo {
    calls: Rc<Cell<usize>>,
    result: Option<SourceInspection>,
    listed: Option<String>,
}
impl SourceInspector for Echo {
    type Inspection<'a> = Ready<Result<SourceInspection, InspectionError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == KIND
    }
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        self.listed.iter().map(SupportedKind::new).collect()
    }
    fn inspect<'a>(&'a self, source: &'a Source, _: CancellationToken) -> Self::Inspection<'a> {
        self.calls.set(self.calls.get() + 1);
        ready(Ok(self.result.clone().unwrap_or_else(|| {
            let mut item = target();
            item.binding.content = source.content.clone();
            SourceInspection::complete(vec![item])
        })))
    }
}
fn inspector(result: Option<SourceInspection>) -> (Inspector<Echo>, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    (
        Inspector::new(Echo {
            calls: calls.clone(),
            result,
            listed: None,
        }),
        calls,
    )
}

#[tokio::test]
async fn exact_support_and_carrier_admission_precede_provider_work() {
    let (inspector, calls) = inspector(None);
    let verdicts = inspector
        .check_kind_support([KIND, "URN:example:echo", "urn:example:echo/2", KIND])
        .unwrap();
    assert_eq!(
        verdicts
            .iter()
            .map(|v| (v.kind.as_str(), v.supported))
            .collect::<Vec<_>>(),
        vec![
            (KIND, true),
            ("URN:example:echo", false),
            ("urn:example:echo/2", false)
        ]
    );
    assert!(inspector.check_kind_support([KIND, ""]).is_err());
    assert!(inspector.check_kind_support(vec![KIND.to_owned()]).unwrap()[0].supported);
    assert!(inspector.list_supported_kinds().unwrap().is_empty());
    assert_eq!(calls.get(), 0);
    for key in [
        "selector",
        "location",
        "kind",
        "content",
        "description",
        "X-unknown",
    ] {
        let mut invalid = source();
        invalid
            .additional_fields
            .insert(key.into(), JsonValue::null());
        assert_eq!(
            inspector.inspect(&invalid).await.unwrap_err().kind(),
            InspectionErrorKind::InvalidInput
        );
    }
    for kind in ["", "URN:example:echo", "urn:example:echo/2"] {
        let invalid = Source {
            kind: kind.into(),
            ..Default::default()
        };
        let kind = inspector.inspect(&invalid).await.unwrap_err().kind();
        assert!(matches!(
            kind,
            InspectionErrorKind::InvalidInput | InspectionErrorKind::UnsupportedKind
        ));
    }
    assert_eq!(calls.get(), 0);
    let mut valid = source();
    valid
        .additional_fields
        .insert("x-provider".into(), JsonValue::null());
    inspector.inspect(&valid).await.unwrap();
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn exact_content_and_absence_survive_source_and_provider_disposal() {
    for text in [
        None,
        Some("null"),
        Some(r#"{"整数":9007199254740993,"名":"空🚲"}"#),
    ] {
        let (inspector, _) = inspector(None);
        let mut source = source();
        source.content = text.map(|text| JsonValue::parse(text).unwrap());
        let result = inspector.inspect(&source).await.unwrap();
        drop(source);
        drop(inspector);
        assert_eq!(
            result.targets[0].binding.content.as_ref().map(|v| v.text()),
            text
        );
        let wire = result.to_json().unwrap();
        let binding = wire
            .view()
            .get("targets")
            .unwrap()
            .elements()
            .unwrap()
            .next()
            .unwrap()
            .get("binding")
            .unwrap();
        assert_eq!(binding.get("content").is_some(), text.is_some());
        if let Some(value) = binding.get("content").and_then(|v| v.get("整数")) {
            assert_eq!(value.number_text(), Some("9007199254740993"));
        }
    }
}

#[tokio::test]
async fn consumer_rejects_provider_author_keys_unknown_binding_fields_and_bad_framing() {
    for key in [
        "operation",
        "source",
        "selector",
        "location",
        "content",
        "idempotent",
    ] {
        let mut item = target();
        item.binding
            .extensions
            .insert(key.into(), JsonValue::null());
        assert!(
            item.binding
                .clone()
                .bind("author", "author-source")
                .is_err()
        );
        assert!(JsonValue::from_serializable(&item.binding).is_err());
        let (inspector, _) = inspector(Some(SourceInspection::complete(vec![item])));
        assert_eq!(
            inspector.inspect(&source()).await.unwrap_err().kind(),
            InspectionErrorKind::InvalidResult
        );
    }
    let mut item = target();
    item.operation = Some(Operation {
        additional_fields: BTreeMap::from([(
            "idempotent".into(),
            JsonValue::parse("true").unwrap(),
        )]),
        ..Default::default()
    });
    let (inspector, _) = inspector(Some(SourceInspection::complete(vec![item])));
    assert_eq!(
        inspector.inspect(&source()).await.unwrap_err().kind(),
        InspectionErrorKind::InvalidResult
    );
    let binding = BindingFragment {
        extensions: BTreeMap::from([("x-provider".into(), JsonValue::null())]),
        ..Default::default()
    }
    .bind("author", "author-source")
    .unwrap();
    assert_eq!(binding.operation, "author");
    assert_eq!(binding.source, "author-source");
    assert!(binding.content.is_none());
}

#[tokio::test]
async fn incomplete_results_require_evidence_and_complete_empty_is_valid() {
    let invalid = [
        incomplete_without_evidence(),
        SourceInspection::partial(vec![], InspectionLimitation::new("", "missing")),
        SourceInspection::partial(vec![], InspectionLimitation::new("example.limit", "")),
        SourceInspection::complete(vec![without_source_ref()]),
    ];
    for result in invalid {
        let (inspector, _) = inspector(Some(result));
        assert_eq!(
            inspector.inspect(&source()).await.unwrap_err().kind(),
            InspectionErrorKind::InvalidResult
        );
    }
    let (inspector, _) = inspector(Some(SourceInspection::complete(vec![])));
    assert!(inspector.inspect(&source()).await.unwrap().exhaustive);
    // The abstract schema permits a limitation on an exhaustive inventory.
    SourceInspection::complete(vec![])
        .with_limitation(InspectionLimitation::new(
            "example.note",
            "complete within the kind's admitted set",
        ))
        .validate()
        .unwrap();
}

#[test]
fn inconsistent_advisory_list_is_rejected() {
    let inspector = Inspector::new(Echo {
        calls: Rc::new(Cell::new(0)),
        result: None,
        listed: Some("urn:other".into()),
    });
    assert_eq!(
        inspector.list_supported_kinds().unwrap_err().kind(),
        InspectionErrorKind::InvalidResult
    );
}

#[test]
fn exact_encoding_budgets_and_opaque_values_are_preserved() {
    let mut item = target();
    // Neither a duplicate name nor a lone UTF-16 surrogate in opaque binding
    // content is normalized by this carrier. A completed document must meet OBI-01.
    let content = r#"{"same":1,"same":2,"surrogate":"\uD800","huge":9007199254740993}"#;
    item.binding.content = Some(JsonValue::parse(content).unwrap());
    let result = SourceInspection::complete(vec![item]);
    let wire = result.to_json().unwrap();
    assert!(wire.text().contains(content));
    assert!(wire.has_duplicate_names());
    let binding = result.targets[0].binding.clone().bind("op", "s").unwrap();
    // Parse the completed carriage so duplicate evidence is retained; the editor
    // correctly refuses inserting an ambiguous object through a typed setter.
    let document = openbindings::Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{}}}},"sources":{{"s":{{"kind":"urn:example:echo"}}}},"bindings":{{"b":{}}}}}"#,
        serde_json::to_string(&binding).unwrap()
    )).unwrap();
    let validation = document.validate().unwrap();
    assert!(validation.conformant().is_none());
    assert_eq!(
        validation.report().evidence["OBI-01"],
        openbindings::Evidence::Violated
    );
    assert_eq!(
        validation.report().evidence["OBI-02"],
        openbindings::Evidence::NotApplicable
    );
    let finding = validation
        .report()
        .findings
        .iter()
        .find(|f| f.code == "unpaired-surrogate")
        .unwrap();
    assert_eq!(finding.rule, "OBI-01");
    assert_eq!(
        finding
            .location
            .as_ref()
            .unwrap()
            .pointer
            .as_ref()
            .and_then(openbindings::Pointer::as_str),
        Some("/bindings/b/content/surrogate")
    );

    let limits = openbindings::JsonLimits {
        max_bytes: wire.text().len(),
        ..Default::default()
    };
    assert!(result.to_json_with_limits(limits).is_ok());
    for limits in [
        openbindings::JsonLimits {
            max_bytes: wire.text().len() - 1,
            ..Default::default()
        },
        openbindings::JsonLimits {
            max_nodes: 1,
            ..Default::default()
        },
        openbindings::JsonLimits {
            max_depth: 1,
            ..Default::default()
        },
    ] {
        assert_eq!(
            result.to_json_with_limits(limits).unwrap_err().kind(),
            InspectionErrorKind::Encoding
        );
    }
    let invalid = incomplete_without_evidence();
    assert_eq!(
        invalid.to_json().unwrap_err().kind(),
        InspectionErrorKind::InvalidResult
    );
}

#[tokio::test]
async fn optional_operation_examples_cannot_shadow_typed_members() {
    let mut item = target();
    item.operation = Some(Operation {
        examples: Some(BTreeMap::from([(
            "sample".into(),
            openbindings::Example {
                additional_fields: BTreeMap::from([("input".into(), JsonValue::null())]),
                ..Default::default()
            },
        )])),
        ..Default::default()
    });
    let (inspector, _) = inspector(Some(SourceInspection::complete(vec![item])));
    assert_eq!(
        inspector.inspect(&source()).await.unwrap_err().kind(),
        InspectionErrorKind::InvalidResult
    );
}

struct Waiting {
    dropped: Rc<Cell<bool>>,
    started: Rc<Cell<bool>>,
}
struct Wait {
    dropped: Rc<Cell<bool>>,
}
impl Future for Wait {
    type Output = Result<SourceInspection, InspectionError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for Wait {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}
impl SourceInspector for Waiting {
    type Inspection<'a> = Wait;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == KIND
    }
    fn inspect<'a>(&'a self, _: &'a Source, _: CancellationToken) -> Self::Inspection<'a> {
        self.started.set(true);
        Wait {
            dropped: self.dropped.clone(),
        }
    }
}

#[tokio::test]
async fn pre_cancelled_work_never_calls_provider() {
    let (inspector, calls) = inspector(None);
    let control = CancellationToken::new();
    control.cancel();
    assert_eq!(
        inspector
            .inspect_with_control(&source(), &control)
            .await
            .unwrap_err()
            .kind(),
        InspectionErrorKind::Cancelled
    );
    assert_eq!(calls.get(), 0);
}

#[tokio::test]
async fn constructing_or_dropping_unpolled_work_does_not_call_provider() {
    let (inspector, calls) = inspector(None);
    let source = source();
    let control = CancellationToken::new();
    let ordinary = inspector.inspect(&source);
    let controlled = inspector.inspect_with_control(&source, &control);
    assert_eq!(calls.get(), 0);
    drop(ordinary);
    assert_eq!(calls.get(), 0);
    control.cancel();
    assert_eq!(
        controlled.await.unwrap_err().kind(),
        InspectionErrorKind::Cancelled
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn cancellation_drops_pending_local_provider_future() {
    let dropped = Rc::new(Cell::new(false));
    let started = Rc::new(Cell::new(false));
    let inspector = Inspector::new(Waiting {
        dropped: dropped.clone(),
        started: started.clone(),
    });
    let source = source();
    let control = CancellationToken::new();
    let mut future = pin!(inspector.inspect_with_control(&source, &control));
    let wakes = Arc::new(WakeCounter(AtomicUsize::new(0)));
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert!(started.get());
    let other_dropped = Rc::new(Cell::new(false));
    let other = Inspector::new(Waiting {
        dropped: other_dropped.clone(),
        started: Rc::new(Cell::new(false)),
    });
    let mut other_future = pin!(other.inspect_with_control(&source, &control));
    assert!(other_future.as_mut().poll(&mut cx).is_pending());
    control.cancel();
    assert_eq!(
        wakes.0.load(Ordering::SeqCst),
        2,
        "each pending facade must be woken"
    );
    let Poll::Ready(Err(error)) = future.as_mut().poll(&mut cx) else {
        panic!("cancellation did not finish")
    };
    assert_eq!(error.kind(), InspectionErrorKind::Cancelled);
    assert!(dropped.get());
    assert!(
        matches!(other_future.as_mut().poll(&mut cx), Poll::Ready(Err(error)) if error.kind() == InspectionErrorKind::Cancelled)
    );
    assert!(other_dropped.get());
}

struct WakeCounter(AtomicUsize);
impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn local_rc_provider_is_forwarded_without_send_requirements() {
    let calls = Rc::new(Cell::new(0));
    let provider = Rc::new(Echo {
        calls: calls.clone(),
        result: None,
        listed: Some(KIND.into()),
    });
    let inspector = Inspector::new(provider.clone());
    assert!(inspector.check_kind_support([KIND]).unwrap()[0].supported);
    assert_eq!(inspector.list_supported_kinds().unwrap()[0].kind, KIND);
    inspector.inspect(&source()).await.unwrap();
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn every_inventory_output_path_refuses_the_same_invalid_results() {
    let mut forbidden_framing = target();
    forbidden_framing.operation = Some(Operation {
        additional_fields: BTreeMap::from([(
            "idempotent".into(),
            JsonValue::parse("true").unwrap(),
        )]),
        ..Default::default()
    });
    let empty_id = without_source_ref();
    for item in [&forbidden_framing, &empty_id] {
        assert!(
            serde_json::to_string(item).is_err(),
            "standalone target bypassed validation"
        );
    }
    let invalid = [
        incomplete_without_evidence(),
        SourceInspection::partial(vec![], InspectionLimitation::new("", "why")),
        SourceInspection::complete(vec![forbidden_framing]),
        SourceInspection::complete(vec![empty_id]),
        SourceInspection::complete(vec![target(), target()]),
    ];
    for inventory in invalid {
        assert!(inventory.validate().is_err());
        assert!(inventory.to_json().is_err());
        assert!(serde_json::to_string(&inventory).is_err());
        let (inspector, _) = inspector(Some(inventory));
        assert_eq!(
            inspector.inspect(&source()).await.unwrap_err().kind(),
            InspectionErrorKind::InvalidResult
        );
    }
}

#[test]
fn source_ref_uniqueness_is_exact_and_valid_serializers_agree() {
    let mut targets = Vec::new();
    for id in ["target", "TARGET", "é", "e\u{301}"] {
        let mut item = target();
        item.source_ref = id.into();
        item.binding.content = Some(JsonValue::parse("9007199254740993").unwrap());
        targets.push(item);
    }
    let inventory = SourceInspection::complete(targets);
    assert!(inventory.validate().is_ok());
    assert_eq!(
        inventory.to_json().unwrap().text(),
        serde_json::to_string(&inventory).unwrap()
    );
}

// every operation extension obeys the same carrier rule as binding and
// source extensions. Neither standalone serialization nor inspection bypasses it.
#[tokio::test]
async fn operation_framing_refuses_unknown_non_extension_members() {
    for key in ["foo", "idempotent", "input", "x-evidence"] {
        let item = target().with_operation(Operation {
            additional_fields: BTreeMap::from([(key.into(), JsonValue::parse("1e9999").unwrap())]),
            ..Default::default()
        });
        let valid = key.starts_with("x-");
        assert_eq!(serde_json::to_string(&item).is_ok(), valid);
        let inventory = SourceInspection::complete(vec![item]);
        assert_eq!(inventory.to_json().is_ok(), valid);
        assert_eq!(serde_json::to_string(&inventory).is_ok(), valid);
        let (facade, _) = inspector(Some(inventory));
        assert_eq!(facade.inspect(&source()).await.is_ok(), valid);
    }
}

// keep crate-local forwarding and bounded writers while there is no
// shared private package boundary. Exercise their composition to prevent drift.
#[tokio::test]
async fn forwarded_inspectors_preserve_exact_encoding_and_byte_limits() {
    struct Provider;
    impl SourceInspector for Provider {
        type Inspection<'a> = Ready<Result<SourceInspection, InspectionError>>;
        fn supports_kind(&self, kind: &str) -> bool {
            kind == KIND
        }
        fn inspect<'a>(&'a self, _: &'a Source, _: CancellationToken) -> Self::Inspection<'a> {
            ready(Ok(SourceInspection::complete(vec![target()])))
        }
    }
    let provider = std::sync::Arc::new(Provider);
    let shared = Inspector::new(provider.clone());
    let borrowed = Inspector::new(&*provider);
    let local = Inspector::new(Rc::new(provider.clone()));
    let source = source();
    for facade_result in [
        shared.inspect(&source).await,
        borrowed.inspect(&source).await,
        local.inspect(&source).await,
    ] {
        let result = facade_result.unwrap();
        let wire = result.to_json().unwrap();
        assert!(
            result
                .to_json_with_limits(JsonLimits {
                    max_bytes: wire.bytes().len(),
                    ..Default::default()
                })
                .is_ok()
        );
        assert!(
            result
                .to_json_with_limits(JsonLimits {
                    max_bytes: wire.bytes().len() - 1,
                    ..Default::default()
                })
                .is_err()
        );
    }
}

#[test]
fn provider_constructors_fill_the_fields_they_name() {
    let kind = SupportedKind::new(KIND).with_description("echo values");
    assert_eq!(
        (kind.kind.as_str(), kind.description.as_deref()),
        (KIND, Some("echo values"))
    );
    assert!(SupportedKind::new(KIND).description.is_none());
    let operation = Operation {
        input: Some(JsonValue::parse("true").unwrap()),
        ..Default::default()
    };
    let target = BindableTarget::new("one", BindingFragment::default())
        .with_operation_key("get")
        .with_operation(operation);
    assert_eq!(target.source_ref, "one");
    assert_eq!(target.operation_key.as_deref(), Some("get"));
    assert!(target.operation.is_some());
    let evidence = BTreeMap::from([("count".to_owned(), JsonValue::parse("2").unwrap())]);
    let limitation = InspectionLimitation::new("example.note", "two records were skipped")
        .with_details(evidence.clone())
        .with_additional_fields(BTreeMap::from([("x-hint".to_owned(), JsonValue::null())]));
    assert_eq!(limitation.details.as_ref().map(BTreeMap::len), Some(1));
    // Evidence on a complete inventory keeps it complete.
    let complete =
        SourceInspection::complete(vec![target.clone()]).with_limitation(limitation.clone());
    assert!(complete.exhaustive);
    assert_eq!(
        complete.to_json().unwrap().text(),
        r#"{"targets":[{"sourceRef":"one","binding":{},"operationKey":"get","operation":{"input":true}}],"exhaustive":true,"limitation":{"code":"example.note","message":"two records were skipped","details":{"count":2},"x-hint":null}}"#
    );
    let partial = SourceInspection::partial(vec![target], limitation);
    assert!(!partial.exhaustive && partial.limitation.is_some());
    assert!(partial.validate().is_ok());
}
