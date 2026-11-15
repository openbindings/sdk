use openbindings::{Document, JsonValue, Pointer};
use openbindings_source_pull::{
    CancellationToken, PullError, PullFailure, PullLimits, PullRequest, PullResult,
    PullValidationError, RelationshipErrorKind as Kind, SourcePuller, SourcePullerExt,
};
use serde_json::{Value, json};
use std::{
    future::{Future, Ready, ready},
    sync::atomic::AtomicBool,
    task::{Context, Poll, Waker},
};

fn fixture() -> (Value, Value) {
    let document = json!({
        "openbindings":"0.2.0", "operations":{"o":{"aliases":["reserved"]}},
        "schemas":{"S":true,"T":false}, "sources":{"s":{"kind":"k"},"other":{"kind":"k"}},
        "bindings":{
            "a":{"source":"s","operation":"o"},
            "b":{"source":"s","operation":"o"},
            "c":{"source":"s","operation":"o"}
        }
    });
    let result = json!({
        "additions":{
            "operations":{"n0":{},"n1":{},"n2":{}},
            "bindings":{
                "d":{"source":"s","operation":"n0"},
                "e":{"source":"s","operation":"n1"},
                "f":{"source":"s","operation":"n2"}
            }
        },
        "report":{"exhaustive":true,
            "added":[{"binding":"d","operation":"n0"},{"binding":"e","operation":"n1"},{"binding":"f","operation":"n2"}],
            "existing":[{"binding":"a","status":"current"},{"binding":"b","status":"current"},{"binding":"c","status":"current"}],
            "omitted":[
                {"status":"invalid","reasonCode":"r","message":"PRIVATE_REPORT_VALUE"},
                {"status":"invalid","reasonCode":"r","message":"PRIVATE_REPORT_VALUE"},
                {"status":"invalid","reasonCode":"r","message":"PRIVATE_REPORT_VALUE"}
            ]
        }
    });
    (document, result)
}
fn request(document: &Value) -> PullRequest {
    PullRequest::new(
        Document::parse(document.to_string())
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap(),
        "s",
    )
}
fn result(value: &Value) -> PullResult {
    // Ordinary serde_json cannot represent an unpaired UTF-16 unit.
    let text = value.to_string().replace("IDENTIFIER", r"\ud800");
    PullResult::from_json(JsonValue::parse(text).unwrap()).unwrap()
}
fn complete<F: Future>(future: F) -> F::Output {
    match Box::pin(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("immediate provider"),
    }
}
struct Provider(PullResult);
impl SourcePuller for Provider {
    type Error = ();
    type Pull<'a> = Ready<Result<PullResult, PullFailure<()>>>;
    fn handle_pull_source(&self, _: PullRequest, _: CancellationToken) -> Self::Pull<'_> {
        ready(Ok(self.0.clone()))
    }
}
fn assert_location(error: PullValidationError, kind: Kind, pointer: Option<&str>) {
    let PullValidationError::Relationship(error) = error else {
        panic!("relationship: {error:?}")
    };
    fn is_copy<T: Copy>(_: T) {}
    is_copy(error.kind());
    assert_eq!(error.kind(), kind);
    assert_eq!(error.pointer().and_then(Pointer::as_str), pointer);
    assert_eq!(error.clone(), error);
    assert!(!format!("{error} {error:?}").contains("PRIVATE_REPORT_VALUE"));
    assert!(!error.to_string().contains("/report"));
}

#[test]
fn every_relationship_category_preserves_its_report_row_through_checked_calls_and_mapping() {
    for (kind, pointer) in [
        (Kind::Identifier, Some("/report/added/1")),
        (Kind::KeyCollision, Some("/report/added/1")),
        (Kind::BindingSource, Some("/report/added/1")),
        (Kind::AddedCoverage, Some("/report/added/1")),
        (Kind::BindingOperation, Some("/report/added/1")),
        (Kind::Reuse, Some("/report/added/1")),
        (Kind::ExistingCoverage, None),
        (Kind::ExistingTarget, Some("/report/existing/1")),
        (Kind::ReferenceTarget, Some("/report/existing/1")),
        (Kind::OmittedTarget, Some("/report/omitted/1")),
        (Kind::DuplicateTarget, Some("/report/existing/1")),
        (Kind::Uses, Some("/report/added/1")),
    ] {
        let (mut document, mut value) = fixture();
        result(&value)
            .validate_for(
                &request(&document),
                &PullLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        match kind {
            Kind::Identifier => value["report"]["added"][1]["binding"] = json!("IDENTIFIER"),
            Kind::KeyCollision => document["operations"]["n1"] = json!({}),
            Kind::BindingSource => value["additions"]["bindings"]["e"]["source"] = json!("other"),
            Kind::AddedCoverage => {
                value["report"]["added"][1] = value["report"]["added"][0].clone()
            }
            Kind::BindingOperation => value["report"]["added"][1]["operation"] = json!("n0"),
            Kind::Reuse => value["report"]["added"][1]["reused"] = json!(true),
            Kind::ExistingCoverage => {
                value["report"]["existing"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            }
            Kind::ExistingTarget => value["report"]["existing"][1]["binding"] = json!("absent"),
            Kind::ReferenceTarget => {
                value["report"]["existing"][1]["references"] = json!(["schemas/absent"])
            }
            Kind::OmittedTarget => {
                value["report"]["omitted"][1]["existing"] = json!("schemas/absent")
            }
            Kind::DuplicateTarget => value["report"]["existing"][1]["binding"] = json!("a"),
            Kind::Uses => value["report"]["added"][1]["uses"] = json!(["bindings/e"]),
            _ => panic!("test category"),
        }
        let request = request(&document);
        let provider = Provider(result(&value));
        let exact_report = provider.0.value().value().text().to_owned();
        let error = provider
            .0
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap_err();
        assert_location(error, kind, pointer);
        let error = complete(provider.pull_source(request.clone(), CancellationToken::new()))
            .unwrap_err()
            .map_error(|_| -> String { panic!("local error does not call mapper") });
        let PullError::Validation(error) = error else {
            panic!("validation")
        };
        assert_location(error, kind, pointer);
        assert_eq!(provider.0.value().value().text(), exact_report);
        assert_eq!(
            std::str::from_utf8(request.interface().document().as_bytes()).unwrap(),
            document.to_string()
        );
    }
}

#[test]
fn relationship_failures_without_a_report_row_are_unlocated() {
    let (document, mut value) = fixture();
    value["report"]["added"].as_array_mut().unwrap().remove(1);
    assert_location(
        result(&value)
            .validate_for(
                &request(&document),
                &PullLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap_err(),
        Kind::AddedCoverage,
        None,
    );
    let (mut document, value) = fixture();
    document["schemas"]["collision"] = json!(true);
    let mut value = value;
    value["additions"]["schemas"] = json!({"collision":false});
    value["report"]["added"][1]["schema"] = json!("collision");
    assert_location(
        result(&value)
            .validate_for(
                &request(&document),
                &PullLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap_err(),
        Kind::KeyCollision,
        None,
    );
}

#[test]
fn nested_proposal_uses_and_alias_collisions_locate_the_report_row() {
    let (document, mut value) = fixture();
    value["report"]["existing"][1] = json!({"binding":"b","status":"stale","change":"dangerous","proposal":{"data":null,"uses":["schemas/absent"]}});
    assert_location(
        result(&value)
            .validate_for(
                &request(&document),
                &PullLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap_err(),
        Kind::Uses,
        Some("/report/existing/1"),
    );
    let (document, mut value) = fixture();
    let operation = value["additions"]["operations"]
        .as_object_mut()
        .unwrap()
        .remove("n1")
        .unwrap();
    value["additions"]["operations"]["reserved"] = operation;
    value["additions"]["bindings"]["e"]["operation"] = json!("reserved");
    value["report"]["added"][1]["operation"] = json!("reserved");
    assert_location(
        result(&value)
            .validate_for(
                &request(&document),
                &PullLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap_err(),
        Kind::KeyCollision,
        Some("/report/added/1"),
    );
}

#[test]
fn locating_an_addition_collision_obeys_work_limits_and_cancellation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CancelAfter(AtomicUsize);
    impl openbindings::WorkControl for CancelAfter {
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 100
        }
    }
    let document =
        json!({"openbindings":"0.2.0","operations":{"o":{}},"sources":{"s":{"kind":"k"}}});
    let rows: Vec<_> = (0..2000)
        .map(|index| json!({"binding":format!("b{index}"),"operation":"o"}))
        .collect();
    let value =
        json!({"additions":{"operations":{"o":{}}},"report":{"exhaustive":true,"added":rows}});
    let result = result(&value);
    let request = request(&document);
    assert_location(
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap_err(),
        Kind::KeyCollision,
        Some("/report/added/1999"),
    );
    assert!(matches!(
        result.validate_for(
            &request,
            &PullLimits::default(),
            &CancelAfter(AtomicUsize::new(0))
        ),
        Err(PullValidationError::Cancelled)
    ));
    // Admission charges both envelopes before relationship work. Allow that
    // and ample index setup, then require the location search to honor its cap.
    let limits = PullLimits {
        max_relationship_work: Some(document.to_string().len() + value.to_string().len() + 1000),
        ..PullLimits::default()
    };
    assert!(matches!(
        result.validate_for(&request, &limits, &AtomicBool::new(false)),
        Err(PullValidationError::LimitExceeded("relationship-work"))
    ));
}
