use openbindings::Document;
use openbindings_source_pull::{CancellationToken, PullRequest, SourcePullerExt};
use source_pull_consumer::Fixed;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn main() {
    let original = Document::parse(r#"{"openbindings":"0.2.0","operations":{},"sources":{"fixed":{"kind":"example.fixed@1"}}}"#)
        .unwrap().validate().unwrap().conformant().unwrap();
    let client = Fixed;
    let mut work = Box::pin(client.pull_source(
        PullRequest::new(original.clone(), "fixed"),
        CancellationToken::new(),
    ));
    let mut context = Context::from_waker(Waker::noop());
    assert!(work.as_mut().poll(&mut context).is_pending());
    let Poll::Ready(Ok(result)) = work.as_mut().poll(&mut context) else {
        panic!("local observation")
    };
    assert!(result.report().is_complete());
    assert_eq!(result.report().added().len(), 1);
    assert!(original.value().get("bindings").is_none());
    println!("Observed one complete addition; the original document remains unchanged.");
}
