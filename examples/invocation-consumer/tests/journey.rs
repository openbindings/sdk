//! One caller crossing the current interfaces, with explicit application decisions.
use invocation_consumer::Catalog;
use openbindings::{
    ConformantDocument, Document, Edit, ExternalSchemas, JsonValue, PreparationLimits, ValueOutcome,
};
use openbindings_invocation::{
    CancellationToken, Context, DefaultOperationInvoker, InvocationError, OperationRequest,
    OutputEvent,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_kind_description::{
    CheckFailure, CheckSourceRequest, DescribeKindRequest, KindDescriptorExt, KindError,
    SourceCheckerExt,
};
use openbindings_source_pull::{
    ExistingStatus, Proposal, PullLimits, PullRequest, PullResult, SourcePullerExt,
};
use source_pull_consumer::{Embedded, Fixed, KindCatalog};
use std::sync::Arc;

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn proof(document: Document) -> ConformantDocument {
    document.validate().unwrap().conformant().unwrap()
}
async fn pull(document: ConformantDocument, embedded: bool) -> PullResult {
    let request = PullRequest::new(document, "catalog");
    if embedded {
        (Embedded)
            .pull_source(request, CancellationToken::new())
            .await
            .unwrap()
    } else {
        (Fixed)
            .pull_source(request, CancellationToken::new())
            .await
            .unwrap()
    }
}
async fn invoke(
    provider: Arc<Catalog>,
    document: ConformantDocument,
    context: Option<Context>,
    value: JsonValue,
) -> (Vec<JsonValue>, Result<(), InvocationError>) {
    let client = DefaultOperationInvoker::new(
        provider,
        DefaultEvaluator::new(),
        ExternalSchemas::default(),
    );
    let mut request = OperationRequest::binding(document, "echo.catalog");
    if let Some(context) = context {
        request = request.with_context(context);
    }
    let (mut input, mut output) = client
        .invoke(request, CancellationToken::new())
        .unwrap()
        .split();
    let sending = async {
        input.send(value).await?;
        input.close();
        Ok::<_, openbindings_invocation::WriteError>(())
    };
    let receiving = async {
        let mut values = Vec::new();
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(value) => values.push(value),
                OutputEvent::Complete => return (values, Ok(())),
                OutputEvent::Error(error) => return (values, Err(error)),
                _ => {}
            }
        }
        (values, Err(InvocationError::transport_closed()))
    };
    let (sent, result) = tokio::join!(sending, receiving);
    if result.1.is_ok() {
        sent.unwrap();
    }
    result
}

#[tokio::test]
async fn two_local_kinds_cross_description_authoring_pull_drift_context_and_invocation() {
    let kinds = KindCatalog::default();
    let listed = kinds.list_kinds(CancellationToken::new()).await.unwrap();
    assert_eq!(listed.len(), 2);
    for embedded in [false, true] {
        let kind = if embedded {
            "example.embedded@1"
        } else {
            "example.fixed@1"
        };
        assert!(
            listed
                .iter()
                .any(|entry| entry.kind().as_str() == Some(kind))
        );
        let checks = kinds.checks();
        let description = kinds
            .describe_kind(
                DescribeKindRequest::from_json(json(&format!(r#"{{"kind":"{kind}"}}"#))).unwrap(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(kinds.checks(), checks, "queries cannot implicitly check");
        assert_eq!(
            description
                .supports()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "openbindings.kind-descriptor.checkSource",
                "openbindings.source-puller.pullSource"
            ]
            .map(str::to_owned)
            .into_iter()
            .collect()
        );
        // Application decision: this fixture discloses a pure carriage check.
        assert!(!description.check().unwrap().executes());
        assert_eq!(description.source_data().is_some(), embedded);
        let proposed = if embedded {
            json(
                r##"{"kind":"example.embedded@1","context":{"callerOnly":9007199254740993},"data":{"opaque":9007199254740993,"items":[{"key":"echo","operation":{"input":{"$id":"https://fixture.test/echo-input","$defs":{"Text":{"type":"string"}},"$ref":"#/$defs/Text"},"output":{"type":"string"}},"data":{"item":"echo","revision":1}},{"key":"other","operation":{"input":{"type":"number"},"output":{"type":"number"}},"data":{"item":"other"}}]}}"##,
            )
        } else {
            json(r#"{"kind":"example.fixed@1","context":{"callerOnly":true}}"#)
        };
        let checked = kinds
            .check_source(
                CheckSourceRequest::from_json(proposed).unwrap(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(kinds.checks(), checks + 1);
        assert!(checked.value().value().get("context").is_none());
        let draft = Document::parse(r#"{"openbindings":"0.2.0","operations":{}}"#).unwrap();
        let original = proof(
            draft
                .edit(&[Edit::set(
                    &["sources"],
                    json(&format!(
                        r#"{{"catalog":{}}}"#,
                        checked.value().value().text()
                    )),
                )])
                .unwrap(),
        );
        assert!(draft.value().get("sources").is_none());
        if embedded {
            assert_eq!(
                original
                    .source("catalog")
                    .unwrap()
                    .data()
                    .unwrap()
                    .get("opaque")
                    .unwrap()
                    .text(),
                "9007199254740993"
            );
        }
        let result = pull(original.clone(), embedded).await;
        assert!(result.report().is_complete());
        assert_eq!(result.report().added().len(), if embedded { 2 } else { 1 });
        let chosen = result
            .report()
            .added()
            .find(|entry| entry.binding().as_str() == Some("echo.catalog"))
            .unwrap();
        assert!(
            chosen.uses().is_none_or(|uses| uses.len() == 0),
            "selected subset must be closed"
        );
        let operation = result
            .additions()
            .operations()
            .unwrap()
            .value()
            .get("echo")
            .unwrap()
            .to_owned();
        let binding = result
            .additions()
            .bindings()
            .unwrap()
            .value()
            .get("echo.catalog")
            .unwrap()
            .to_owned();
        // Only this edit adopts anything; the unselected operation stays absent.
        let adopted = proof(
            original
                .document()
                .edit(&[
                    Edit::set(&["operations", "echo"], operation),
                    Edit::set(
                        &["bindings"],
                        json(&format!(r#"{{"echo.catalog":{}}}"#, binding.text())),
                    ),
                ])
                .unwrap(),
        );
        assert!(original.operations().is_empty());
        assert!(original.value().get("bindings").is_none());
        assert!(adopted.operation("other").is_none());
        let again = pull(adopted.clone(), embedded).await;
        assert_eq!(again.report().added().len(), usize::from(embedded));
        assert!(
            again
                .report()
                .added()
                .all(|entry| entry.binding().as_str() != Some("echo.catalog"))
        );
        let current = again.report().existing().unwrap().collect::<Vec<_>>();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].status(), ExistingStatus::Current);

        let active = if embedded {
            let changed = proof(
                adopted
                    .document()
                    .edit(&[Edit::set(
                        &[
                            "sources", "catalog", "data", "items", "0", "data", "revision",
                        ],
                        json("2"),
                    )])
                    .unwrap(),
            );
            let drift = pull(changed.clone(), true).await;
            let entries = drift.report().existing().unwrap().collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].status(), ExistingStatus::Stale);
            let Some(Proposal::Binding(proposal)) = entries[0].proposal() else {
                panic!("binding proposal");
            };
            assert_eq!(
                proposal.data().unwrap().get("revision").unwrap().text(),
                "2"
            );
            let prospective = drift
                .validate_for(
                    &PullRequest::new(changed.clone(), "catalog"),
                    &PullLimits::default(),
                    &CancellationToken::new(),
                )
                .unwrap();
            for document in [&adopted, &changed, &prospective] {
                assert_eq!(
                    document
                        .binding("echo.catalog")
                        .unwrap()
                        .data()
                        .unwrap()
                        .get("revision")
                        .unwrap()
                        .text(),
                    "1",
                    "a pull never applies a proposal"
                );
            }
            changed
        } else {
            adopted
        };
        let provider = Arc::new(Catalog::requiring_context());
        let (values, challenge) =
            invoke(provider.clone(), active.clone(), None, json(r#""hello""#)).await;
        assert!(values.is_empty());
        let challenge = challenge.unwrap_err();
        assert_eq!(challenge.code(), "CONTEXT_REQUIRED");
        assert!(challenge.data().unwrap().get("schema").is_some());
        assert_eq!(provider.values(), 0);
        let context =
            Context::try_from(json(r#"{"permit":"approved","opaque":9007199254740993}"#)).unwrap();
        let (values, invalid) = invoke(
            provider.clone(),
            active.clone(),
            Some(context.clone()),
            json("42"),
        )
        .await;
        assert!(values.is_empty());
        assert_eq!(
            invalid.unwrap_err().code(),
            "ERR_OPERATION_VALIDATION_FAILED"
        );
        assert_eq!(provider.values(), 0);
        let (values, terminal) = invoke(
            provider.clone(),
            active.clone(),
            Some(context),
            json(r#""hello""#),
        )
        .await;
        terminal.unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].text(), r#""hello""#);
        assert_eq!(provider.values(), 1);
        assert_eq!(provider.predictions(), 0, "no implicit preflight");
        assert!(
            active
                .source("catalog")
                .unwrap()
                .value()
                .get("context")
                .is_none()
        );
    }
}

fn accepts(schema: &JsonValue, value: &JsonValue) -> bool {
    let document = proof(
        Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"check":{{"input":{}}}}}}}"#,
            schema.text()
        ))
        .unwrap(),
    );
    let prepared = document
        .operation("check")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::new(),
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
        )
        .unwrap();
    match prepared.evaluate(value) {
        ValueOutcome::Satisfies => true,
        ValueOutcome::Fails { .. } => false,
        other => panic!("description schema did not decide its carriage: {other:?}"),
    }
}

#[tokio::test]
async fn described_data_shapes_agree_with_the_kind_carriage_reader() {
    let kinds = KindCatalog::default();
    let description = kinds
        .describe_kind(
            DescribeKindRequest::from_json(json(r#"{"kind":"example.embedded@1"}"#)).unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let schema = description.source_data().unwrap();
    for (text, expected) in [
        (r#"{"items":[]}"#, true),
        (
            r#"{"items":[{"key":"echo","operation":{"input":true},"data":{"item":"echo"}}]}"#,
            true,
        ),
        // Carriage checking deliberately defers offered OBI entry admission.
        (
            r#"{"items":[{"key":"echo","operation":null,"data":null}]}"#,
            true,
        ),
        ("null", false),
        ("{}", false),
        (r#"{"items":null}"#, false),
        (r#"{"items":[null]}"#, false),
        (r#"{"items":[{}]}"#, false),
        (r#"{"items":[{"key":0,"operation":{},"data":{}}]}"#, false),
        (r#"{"items":[{"key":"echo","operation":{}}]}"#, false),
        (r#"{"items":[{"key":"echo","data":{}}]}"#, false),
        (
            r#"{"items":[{"key":"空🚲","operation":{},"data":{}}]}"#,
            true,
        ),
    ] {
        assert_eq!(accepts(&schema, &json(text)), expected, "{text}");
        let result = kinds
            .check_source(
                CheckSourceRequest::from_json(json(&format!(
                    r#"{{"kind":"example.embedded@1","data":{text}}}"#
                )))
                .unwrap(),
                CancellationToken::new(),
            )
            .await;
        match (expected, result) {
            (true, Ok(checked)) => assert_eq!(checked.data().unwrap().text(), text),
            (false, Err(KindError::Provider(CheckFailure::Rejected(rejection)))) => {
                assert_eq!(rejection.code().as_str(), Some("example.invalid-source"))
            }
            (_, other) => panic!("description/check disagreement for {text}: {other:?}"),
        }
    }
    let fixed = kinds
        .describe_kind(
            DescribeKindRequest::from_json(json(r#"{"kind":"example.fixed@1"}"#)).unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let binding = fixed.binding_data().unwrap();
    for (data, expected) in [
        (r#"{"item":"echo"}"#, true),
        (r#"{"item":"not-echo"}"#, false),
        ("{}", false),
        (r#"{"item":null}"#, false),
    ] {
        assert_eq!(accepts(&binding, &json(data)), expected);
    }
    for text in [
        r#"{"kind":"example.fixed@1","data":null}"#,
        r#"{"kind":"unknown"}"#,
        r#"{"kind":"example.embedded@1","data":{"items":[{"key":"\ud800","operation":{},"data":{}}]}}"#,
        r#"{"kind":"example.embedded@1","data":{"items":[{"key":"\udc00","operation":{},"data":{}}]}}"#,
    ] {
        assert!(matches!(
            kinds
                .check_source(
                    CheckSourceRequest::from_json(json(text)).unwrap(),
                    CancellationToken::new(),
                )
                .await,
            Err(KindError::Provider(CheckFailure::Rejected(_)))
        ));
    }
}
