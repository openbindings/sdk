use openbindings::{Document, Edit, JsonLimits, JsonValue};
use openbindings_source_pull::{
    CancellationToken, PullFailure, PullLimits, PullRequest, PullResult, SourcePuller,
    SourcePullerExt,
};
use std::{
    future::{Future, Ready, ready},
    pin::pin,
    task::{Context, Poll, Waker},
};

struct Provider(PullResult);
impl SourcePuller for Provider {
    type Error = ();
    type Pull<'a> = Ready<Result<PullResult, PullFailure<()>>>;
    fn handle_pull_source(&self, _: PullRequest, _: CancellationToken) -> Self::Pull<'_> {
        ready(Ok(self.0.clone()))
    }
}
fn complete<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready"),
    }
}

#[test]
fn both_checked_methods_return_all_additions_without_applying_stale_proposals() {
    let limits = JsonLimits {
        max_bytes: 4096,
        max_nodes: 128,
        ..Default::default()
    };
    let original = Document::parse_with_limits(r#"{"openbindings":"0.2.0","operations":{"old":{"input":true}},"sources":{"s":{"kind":"example@1"}},"bindings":{"old":{"operation":"old","source":"s","data":1}}}"#, limits)
        .unwrap().validate().unwrap().conformant().unwrap();
    let request = PullRequest::new(original.clone(), "s");
    let provider = Provider(PullResult::from_json(JsonValue::parse(r##"{"additions":{"operations":{"new":{"input":{"$ref":"#/schemas/S"}}},"schemas":{"S":false},"bindings":{"new":{"operation":"new","source":"s"}},"metadata":"private"},"report":{"exhaustive":true,"added":[{"binding":"new","operation":"new"}],"existing":[{"binding":"old","status":"stale","change":"dangerous","proposal":{"input":false,"data":null}}]}}"##).unwrap()).unwrap());
    for custom in [false, true] {
        let (result, prospective) = if custom {
            complete(provider.pull_source_with_limits(
                request.clone(),
                CancellationToken::new(),
                PullLimits::default(),
            ))
            .unwrap()
        } else {
            complete(provider.pull_source(request.clone(), CancellationToken::new())).unwrap()
        };
        assert_eq!(
            result.value().value().text(),
            provider.0.value().value().text()
        );
        assert!(prospective.operation("new").is_some());
        assert!(prospective.binding("new").is_some());
        assert!(
            prospective
                .value()
                .get("schemas")
                .unwrap()
                .get("S")
                .is_some()
        );
        assert_eq!(
            prospective
                .operation("old")
                .unwrap()
                .value()
                .get("input")
                .unwrap()
                .as_bool(),
            Some(true)
        );
        assert_eq!(
            prospective
                .binding("old")
                .unwrap()
                .data()
                .unwrap()
                .view()
                .number_text(),
            Some("1")
        );
        assert!(prospective.value().get("metadata").is_none());
        let retained = prospective.document().limits();
        assert_eq!(
            (retained.max_bytes, retained.max_nodes, retained.max_depth),
            (limits.max_bytes, limits.max_nodes, limits.max_depth)
        );
        let too_large = JsonValue::parse(format!("\"{}\"", "x".repeat(4096))).unwrap();
        assert!(
            prospective
                .document()
                .edit(&[Edit::set(&["description"], too_large)])
                .is_err()
        );
        assert!(original.operation("new").is_none());
        assert!(original.binding("new").is_none());
        assert!(original.value().get("schemas").is_none());
    }
}

#[test]
fn empty_pulls_return_the_original_proof_and_keep_its_exact_document() {
    let original = Document::parse("{ \"openbindings\":\"0.2.0\", \"operations\":{},\"sources\":{\"s\":{\"kind\":\"example@1\"}} }")
        .unwrap().validate().unwrap().conformant().unwrap();
    let request = PullRequest::new(original.clone(), "s");
    let provider = Provider(
        PullResult::from_json(
            JsonValue::parse(r#"{"additions":{},"report":{"exhaustive":true,"added":[]}}"#)
                .unwrap(),
        )
        .unwrap(),
    );
    let (_, prospective) =
        complete(provider.pull_source(request, CancellationToken::new())).unwrap();
    assert_eq!(
        prospective.as_bytes().as_ptr(),
        original.as_bytes().as_ptr()
    );
    assert_eq!(prospective.as_bytes(), original.as_bytes());
}
