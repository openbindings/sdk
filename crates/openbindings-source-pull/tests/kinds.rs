//! Illustrative kind-owned implementations, not new SDK projection abstractions.
//! The two source readers receive no destination document or operation schemas.
use openbindings::{ConformantDocument, Document, Edit, JsonRef, JsonValue};
use openbindings_schema_comparison::{
    ComparisonFailure, ComparisonLimits, Identity, SchemaInput, identical,
};
use openbindings_source_pull::*;
use std::{
    collections::BTreeMap,
    future::{Future, Ready, ready},
    sync::atomic::AtomicBool,
    task::{Context as TaskContext, Poll, Waker},
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap()
}
fn object(fields: BTreeMap<String, JsonValue>) -> JsonValue {
    json(&format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(k, v)| format!("{}:{}", quote(&k), v.text()))
            .collect::<Vec<_>>()
            .join(",")
    ))
}
fn array(values: &[JsonValue]) -> JsonValue {
    json(&format!(
        "[{}]",
        values
            .iter()
            .map(|v| v.text())
            .collect::<Vec<_>>()
            .join(",")
    ))
}
fn field(value: JsonRef<'_>, key: &str) -> JsonValue {
    value.get(key).unwrap().to_owned()
}
fn document(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}
fn complete<F: Future>(future: F) -> F::Output {
    match Box::pin(future)
        .as_mut()
        .poll(&mut TaskContext::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("finite local witness"),
    }
}

#[derive(Debug)]
enum ObservationError {
    Source,
    Comparison(ComparisonFailure),
}
struct Offer {
    key: String,
    operation: JsonValue,
    data: JsonValue,
}
struct Observation {
    offers: Vec<Offer>,
    root: JsonValue,
    exhaustive: bool,
}

// example.fixed@1 has no source data. It describes a fixed echo surface.
fn fixed(source: JsonRef<'_>) -> Result<Observation, ObservationError> {
    if source.get("kind").and_then(|v| v.as_str()) != Some("example.fixed@1")
        || source.get("data").is_some()
    {
        return Err(ObservationError::Source);
    }
    let operation = json(r#"{"input":{"type":"string"},"output":{"type":"string"}}"#);
    Ok(Observation {
        offers: vec![Offer {
            key: "echo".into(),
            operation: operation.clone(),
            data: json(r#"{"item":"echo"}"#),
        }],
        root: operation,
        exhaustive: true,
    })
}
// example.embedded@1 reads only its embedded catalog. Each item owns its offered
// operation schemas and binding data. The catalog may explicitly mark a partial read.
fn embedded(source: JsonRef<'_>) -> Result<Observation, ObservationError> {
    if source.get("kind").and_then(|v| v.as_str()) != Some("example.embedded@1") {
        return Err(ObservationError::Source);
    }
    let root = source
        .get("data")
        .ok_or(ObservationError::Source)?
        .to_owned();
    let items = root
        .get("items")
        .and_then(|v| v.elements())
        .ok_or(ObservationError::Source)?;
    let offers = items
        .map(|item| {
            Ok(Offer {
                key: item
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or(ObservationError::Source)?
                    .into(),
                operation: item
                    .get("operation")
                    .ok_or(ObservationError::Source)?
                    .to_owned(),
                data: item.get("data").ok_or(ObservationError::Source)?.to_owned(),
            })
        })
        .collect::<Result<_, _>>()?;
    let exhaustive = root.get("partial").and_then(|v| v.as_bool()) != Some(true);
    Ok(Observation {
        offers,
        root,
        exhaustive,
    })
}

fn slots(
    offer: &Offer,
    observed: &Observation,
    existing: JsonRef<'_>,
    original: JsonRef<'_>,
) -> Result<[Identity; 2], ObservationError> {
    let mut answers = [Identity::Equal; 2];
    for (index, name) in ["input", "output"].into_iter().enumerate() {
        let left = offer
            .operation
            .get(name)
            .map(|s| SchemaInput::new(s.to_owned(), observed.root.clone()));
        let right = existing
            .get(name)
            .map(|s| SchemaInput::new(s.to_owned(), original.to_owned()));
        answers[index] = identical(left.as_ref(), right.as_ref(), &ComparisonLimits::default())
            .map_err(ObservationError::Comparison)?;
    }
    Ok(answers)
}

fn reconcile(request: &PullRequest, observed: Observation) -> Result<PullResult, ObservationError> {
    let original = request.interface().value();
    let operations = original.get("operations").unwrap();
    let bindings = original.get("bindings");
    let mut new_operations = BTreeMap::new();
    let mut new_bindings = BTreeMap::new();
    let mut added = Vec::new();
    let mut existing = Vec::new();
    let mut omitted = Vec::new();
    for offer in &observed.offers {
        // Existing paths are accounted below, rather than added a second time.
        let accounted = bindings
            .into_iter()
            .flat_map(|v| v.members().unwrap())
            .any(|b| {
                b.value.get("source").and_then(|s| s.as_str()) == Some(request.source())
                    && b.value
                        .get("data")
                        .and_then(|d| d.get("item"))
                        .and_then(|i| i.as_str())
                        == Some(&offer.key)
            });
        if accounted {
            continue;
        }
        let reused = if let Some(operation) = operations.get(&offer.key) {
            let compared = match slots(offer, &observed, operation, original) {
                Ok(compared) => compared,
                Err(ObservationError::Comparison(
                    ComparisonFailure::MissingReference
                    | ComparisonFailure::ReferenceCycle
                    | ComparisonFailure::InvalidReference
                    | ComparisonFailure::UnsupportedReference
                    | ComparisonFailure::UnsatisfiableMerge,
                )) => {
                    omitted.push(json(&format!(r#"{{"status":"implementation-unsupported","reasonCode":"comparison-unfinished","message":"The offered schemas could not be compared.","item":{},"existing":{}}}"#, quote(&format!("operations/{}", offer.key)), quote(&format!("operations/{}", offer.key)))));
                    continue;
                }
                Err(error) => return Err(error),
            };
            if compared != [Identity::Equal; 2] {
                omitted.push(json(&format!(r#"{{"status":"conflict","reasonCode":"offered-schema-differs","message":"The offered input or output differs from the existing operation.","item":{},"existing":{},"offered":{}}}"#, quote(&format!("operations/{}", offer.key)), quote(&format!("operations/{}", offer.key)), offer.operation.text())));
                continue;
            }
            true
        } else {
            false
        };
        let binding = format!("{}.{}", offer.key, request.source());
        new_bindings.insert(
            binding.clone(),
            json(&format!(
                r#"{{"operation":{},"source":{},"data":{}}}"#,
                quote(&offer.key),
                quote(request.source()),
                offer.data.text()
            )),
        );
        if !reused {
            new_operations.insert(offer.key.clone(), offer.operation.clone());
        }
        added.push(json(&format!(
            r#"{{"binding":{},"operation":{}{}}}"#,
            quote(&binding),
            quote(&offer.key),
            if reused { ",\"reused\":true" } else { "" }
        )));
    }
    for binding in bindings.into_iter().flat_map(|v| v.members().unwrap()) {
        if binding.value.get("source").and_then(|s| s.as_str()) != Some(request.source()) {
            continue;
        }
        let key = binding.name.as_str().unwrap();
        let item = binding
            .value
            .get("data")
            .and_then(|d| d.get("item"))
            .and_then(|i| i.as_str());
        let offer = observed
            .offers
            .iter()
            .find(|o| Some(o.key.as_str()) == item);
        let Some(offer) = offer else {
            existing.push(json(&format!(
                r#"{{"binding":{},"status":"unmatched"}}"#,
                quote(key)
            )));
            continue;
        };
        let operation = operations
            .get(binding.value.get("operation").unwrap().as_str().unwrap())
            .unwrap();
        let mut proposal = BTreeMap::new();
        if binding
            .value
            .get("data")
            .map(|v| v.to_owned())
            .as_ref()
            .and_then(|v| v.semantic_eq(&offer.data))
            != Some(true)
        {
            proposal.insert("data".into(), offer.data.clone());
        }
        let comparisons = match slots(offer, &observed, operation, original) {
            Ok(comparisons) => comparisons,
            Err(ObservationError::Comparison(
                ComparisonFailure::MissingReference
                | ComparisonFailure::ReferenceCycle
                | ComparisonFailure::InvalidReference
                | ComparisonFailure::UnsupportedReference
                | ComparisonFailure::UnsatisfiableMerge,
            )) => {
                existing.push(json(&format!(r#"{{"binding":{},"status":"unassessed","reasonCode":"comparison-unfinished","message":"The offered schemas could not be compared."}}"#, quote(key))));
                continue;
            }
            Err(error) => return Err(error),
        };
        let mut remove = Vec::new();
        for (name, comparison) in ["input", "output"].into_iter().zip(comparisons) {
            if comparison == Identity::Different {
                if let Some(value) = offer.operation.get(name) {
                    proposal.insert(name.into(), value.to_owned());
                } else {
                    remove.push(json(&quote(name)));
                }
            }
        }
        if !remove.is_empty() {
            proposal.insert("remove".into(), array(&remove));
        }
        let status = if proposal.is_empty() {
            "\"status\":\"current\"".into()
        } else {
            format!(
                "\"status\":\"stale\",\"change\":\"dangerous\",\"proposal\":{}",
                object(proposal).text()
            )
        };
        existing.push(json(&format!(r#"{{"binding":{},{status}}}"#, quote(key))));
    }
    let additions = object(BTreeMap::from([
        ("operations".into(), object(new_operations)),
        ("bindings".into(), object(new_bindings)),
    ]));
    let limitation = if observed.exhaustive {
        ""
    } else {
        ",\"limitation\":{\"code\":\"partial-catalog\",\"message\":\"Only the supplied catalog portion was observed.\"}"
    };
    PullResult::from_json(json(&format!(r#"{{"additions":{},"report":{{"exhaustive":{}{limitation},"added":{},"existing":{},"omitted":{}}}}}"#, additions.text(), observed.exhaustive, array(&added).text(), array(&existing).text(), array(&omitted).text()))).map_err(|_| ObservationError::Source)
}

#[test]
fn schema_annotations_remain_the_authors_under_profile_identity() {
    for (current, offered, expected) in [
        (
            r#"{"type":"string","title":"old"}"#,
            r#"{"type":"string","title":"new"}"#,
            ExistingStatus::Current,
        ),
        (
            r#"{"type":"string","title":"old"}"#,
            r#"{"title":"old","type":"string"}"#,
            ExistingStatus::Current,
        ),
        (
            r#"{"minimum":1}"#,
            r#"{"minimum":1.0}"#,
            ExistingStatus::Current,
        ),
        (
            r#"{"type":"number","title":"old"}"#,
            r#"{"type":"string","title":"new"}"#,
            ExistingStatus::Stale,
        ),
    ] {
        let original = document(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{"a":{{"input":{current}}}}},"sources":{{"catalog":{{"kind":"example.embedded@1","data":{{"items":[{{"key":"a","operation":{{"input":{offered}}},"data":{{"item":"a"}}}}]}}}}}},"bindings":{{"a":{{"operation":"a","source":"catalog","data":{{"item":"a"}}}}}}}}"#
        ));
        let result = complete((Embedded).pull_source(
            PullRequest::new(original, "catalog"),
            CancellationToken::new(),
        ))
        .unwrap();
        let entry = result.report().existing().unwrap().next().unwrap();
        assert_eq!(entry.status(), expected);
        if expected == ExistingStatus::Stale {
            let Some(Proposal::Binding(proposal)) = entry.proposal() else {
                panic!("binding proposal")
            };
            assert_eq!(
                proposal.input().unwrap().get("title").unwrap().as_str(),
                Some("new")
            );
        }
    }
}

struct Fixed;
struct Embedded;
macro_rules! provider {
    ($name:ty, $project:ident) => {
        impl SourcePuller for $name {
            type Error = ObservationError;
            type Pull<'a> = Ready<Result<PullResult, PullFailure<Self::Error>>>;
            fn handle_pull_source(
                &self,
                request: PullRequest,
                control: CancellationToken,
            ) -> Self::Pull<'_> {
                if control.is_cancelled() {
                    return ready(Err(PullFailure::Cancelled));
                }
                let source = request
                    .interface()
                    .value()
                    .get("sources")
                    .unwrap()
                    .get(request.source())
                    .unwrap();
                ready(
                    $project(source)
                        .and_then(|observed| reconcile(&request, observed))
                        .map_err(PullFailure::Operational),
                )
            }
        }
    };
}
provider!(Fixed, fixed);
provider!(Embedded, embedded);

#[test]
fn no_data_kind_projects_before_destination_identity_and_does_not_adapt_to_conflicts() {
    let client = Fixed;
    for (operations, expected, complete_pull) in [
        ("{}", "new", true),
        (
            r#"{"echo":{"input":{"allOf":[true,{"type":"string"}]},"output":{"type":"string"},"description":"author-owned"}}"#,
            "reused",
            true,
        ),
        (
            r#"{"echo":{"input":{"type":"integer"},"output":{"type":"string"}}}"#,
            "conflict",
            true,
        ),
        (r#"{"echo":{"output":{"type":"string"}}}"#, "conflict", true),
    ] {
        let original = document(&format!(
            r#"{{"openbindings":"0.2.0","operations":{operations},"sources":{{"fixed":{{"kind":"example.fixed@1"}}}}}}"#
        ));
        let result = complete(client.pull_source(
            PullRequest::new(original.clone(), "fixed"),
            CancellationToken::new(),
        ))
        .unwrap();
        assert_eq!(result.report().is_complete(), complete_pull);
        match expected {
            "new" => {
                assert!(!result.report().added().next().unwrap().reused());
                assert!(
                    result
                        .additions()
                        .operations()
                        .unwrap()
                        .value()
                        .get("echo")
                        .is_some()
                );
                let adopted = result
                    .validate_for(
                        &PullRequest::new(original.clone(), "fixed"),
                        &PullLimits::default(),
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                let repeated = complete(
                    client
                        .pull_source(PullRequest::new(adopted, "fixed"), CancellationToken::new()),
                )
                .unwrap();
                assert_eq!(repeated.report().added().len(), 0);
                assert_eq!(
                    repeated
                        .report()
                        .existing()
                        .unwrap()
                        .next()
                        .unwrap()
                        .status(),
                    ExistingStatus::Current
                );
            }
            "reused" => {
                assert!(result.report().added().next().unwrap().reused());
                assert_eq!(
                    result
                        .additions()
                        .operations()
                        .unwrap()
                        .value()
                        .view()
                        .len(),
                    Some(0)
                );
            }
            _ => {
                assert_eq!(result.report().added().len(), 0);
                assert_eq!(
                    result.report().omitted().unwrap().next().unwrap().status(),
                    OmittedStatus::Conflict
                );
            }
        }
        assert!(original.value().get("bindings").is_none());
        assert!(
            original
                .value()
                .get("sources")
                .unwrap()
                .get("fixed")
                .unwrap()
                .get("data")
                .is_none()
        );
    }
}

#[test]
fn two_sources_of_the_same_kind_reuse_one_operation_without_cross_source_suppression() {
    let client = Fixed;
    let original = document(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"first":{"kind":"example.fixed@1"},"second":{"kind":"example.fixed@1"}}}"#,
    );
    let first_request = PullRequest::new(original.clone(), "first");
    let first =
        complete(client.pull_source(first_request.clone(), CancellationToken::new())).unwrap();
    assert!(!first.report().added().next().unwrap().reused());
    let first_snapshot = first
        .validate_for(
            &first_request,
            &PullLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

    let second_request = PullRequest::new(first_snapshot.clone(), "second");
    let second =
        complete(client.pull_source(second_request.clone(), CancellationToken::new())).unwrap();
    assert_eq!(second.report().added().len(), 1);
    assert!(second.report().added().next().unwrap().reused());
    assert_eq!(second.report().existing().unwrap().count(), 0);
    assert_eq!(
        second
            .additions()
            .operations()
            .unwrap()
            .value()
            .view()
            .len(),
        Some(0)
    );
    let both = second
        .validate_for(
            &second_request,
            &PullLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(both.operations().len(), 1);
    assert_eq!(both.operation("echo").unwrap().bindings().len(), 2);
    assert_eq!(both.binding("echo.first").unwrap().source_key(), "first");
    assert_eq!(both.binding("echo.second").unwrap().source_key(), "second");
    assert!(original.value().get("bindings").is_none());
    assert!(first_snapshot.binding("echo.second").is_none());

    let repeated =
        complete(client.pull_source(PullRequest::new(both, "second"), CancellationToken::new()))
            .unwrap();
    assert_eq!(repeated.report().added().len(), 0);
    assert_eq!(repeated.report().existing().unwrap().count(), 1);
    assert_eq!(
        repeated
            .report()
            .existing()
            .unwrap()
            .next()
            .unwrap()
            .status(),
        ExistingStatus::Current
    );
}

#[test]
fn embedded_kind_reports_stale_current_unmatched_and_partial_without_overwriting_authorship() {
    let original = document(
        r#"{"openbindings":"0.2.0","operations":{"a":{"input":{"type":"integer"},"description":"keep"},"b":{"input":true},"c":{}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"partial":true,"items":[{"key":"a","operation":{"input":{"type":"string"}},"data":{"item":"a","revision":"v2"}},{"key":"b","operation":{"input":true},"data":{"item":"b"}}]}}},"bindings":{"a":{"operation":"a","source":"catalog","preference":2,"idempotent":true,"data":{"item":"a","revision":"v1"}},"b":{"operation":"b","source":"catalog","data":{"item":"b"}},"c":{"operation":"c","source":"catalog","data":{"item":"gone"}}}}"#,
    );
    let request = PullRequest::new(original.clone(), "catalog");
    let result =
        complete((Embedded).pull_source(request.clone(), CancellationToken::new())).unwrap();
    assert!(!result.report().is_complete());
    let entries = result.report().existing().unwrap().collect::<Vec<_>>();
    assert_eq!(
        entries.iter().map(|e| e.status()).collect::<Vec<_>>(),
        [
            ExistingStatus::Stale,
            ExistingStatus::Current,
            ExistingStatus::Unmatched
        ]
    );
    let Some(Proposal::Binding(proposal)) = entries[0].proposal() else {
        panic!("binding proposal")
    };
    assert!(proposal.value().value().get("description").is_none());
    // The caller chooses to apply only this proposal, explicitly copying its
    // permitted fields and revalidating. Pulling itself has no such effect.
    let edited = original
        .document()
        .edit(&[
            Edit::set(&["bindings", "a", "data"], proposal.data().unwrap()),
            Edit::set(&["operations", "a", "input"], proposal.input().unwrap()),
        ])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let operation = edited.value().get("operations").unwrap().get("a").unwrap();
    {
        let name = "description";
        assert_eq!(
            field(operation, name).text(),
            field(
                original
                    .value()
                    .get("operations")
                    .unwrap()
                    .get("a")
                    .unwrap(),
                name
            )
            .text()
        );
    }
    for name in ["preference", "idempotent"] {
        let before = original.value().get("bindings").unwrap().get("a").unwrap();
        let after = edited.value().get("bindings").unwrap().get("a").unwrap();
        assert_eq!(field(before, name).text(), field(after, name).text());
    }
    assert_eq!(
        original
            .value()
            .get("operations")
            .unwrap()
            .get("a")
            .unwrap()
            .get("input")
            .unwrap()
            .get("type")
            .unwrap()
            .as_str(),
        Some("integer")
    );
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
}

#[test]
fn a_caller_can_keep_part_of_a_pull_by_following_schema_references() {
    let original = document(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example.catalog@1"}}}"#,
    );
    let request = PullRequest::new(original.clone(), "s");
    let result = PullResult::from_json(json(r##"{"additions":{"operations":{"a":{"input":{"$ref":"#/schemas/S"}},"b":{"input":{"$ref":"#/schemas/T"}}},"schemas":{"S":{"type":"string"},"T":{"type":"integer"}},"bindings":{"a":{"source":"s","operation":"a"},"b":{"source":"s","operation":"b"}}},"report":{"exhaustive":true,"added":[{"binding":"a","operation":"a"},{"binding":"b","operation":"b"}]}}"##)).unwrap();
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    // Inspecting or dropping a retained result changes nothing in the original.
    drop(result.clone());
    assert!(original.value().get("schemas").is_none());
    assert_eq!(original.value().get("operations").unwrap().len(), Some(0));
    let additions = result.additions();
    let operation = additions
        .operations()
        .unwrap()
        .value()
        .get("a")
        .unwrap()
        .to_owned();
    // Follow a's input reference to the schema addition before inserting it.
    assert_eq!(
        operation
            .get("input")
            .unwrap()
            .get("$ref")
            .unwrap()
            .as_str(),
        Some("#/schemas/S")
    );
    let schema = additions
        .schemas()
        .unwrap()
        .value()
        .get("S")
        .unwrap()
        .to_owned();
    let binding = additions
        .bindings()
        .unwrap()
        .value()
        .get("a")
        .unwrap()
        .to_owned();
    let selected = original
        .document()
        .edit(&[
            Edit::set(&["operations", "a"], operation.clone()),
            Edit::set(&["schemas"], object(BTreeMap::from([("S".into(), schema)]))),
            Edit::set(
                &["bindings"],
                object(BTreeMap::from([("a".into(), binding)])),
            ),
        ])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    assert!(
        selected
            .value()
            .get("operations")
            .unwrap()
            .get("b")
            .is_none()
    );
    assert!(selected.value().get("schemas").unwrap().get("T").is_none());
    // Missing the schema reference target is caught by revalidation of this chosen
    // edit; validation of the all-additions snapshot did not authorize any subset.
    let incomplete = original
        .document()
        .edit(&[Edit::set(&["operations", "a"], operation)])
        .unwrap()
        .validate()
        .unwrap();
    assert!(incomplete.conformant().is_none());
}

#[test]
fn comparison_gaps_and_removal_proposals_are_reported() {
    let client = Embedded;
    let comparison = document(
        r##"{"openbindings":"0.2.0","operations":{"a":{"input":true}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"items":[{"key":"a","operation":{"input":{"$ref":"#/missing"}},"data":{"item":"a"}}]}}}}"##,
    );
    let result = complete(client.pull_source(
        PullRequest::new(comparison, "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    assert_eq!(
        result.report().omitted().unwrap().next().unwrap().status(),
        OmittedStatus::ImplementationUnsupported
    );
    assert!(!result.report().is_complete());
    let removal = document(
        r#"{"openbindings":"0.2.0","operations":{"a":{"input":true}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"items":[{"key":"a","operation":{},"data":{"item":"a"}}]}}},"bindings":{"a":{"operation":"a","source":"catalog","data":{"item":"a"}}}}"#,
    );
    let result = complete(client.pull_source(
        PullRequest::new(removal.clone(), "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    let entry = result.report().existing().unwrap().next().unwrap();
    assert_eq!(entry.status(), ExistingStatus::Stale);
    let Some(Proposal::Binding(proposal)) = entry.proposal() else {
        panic!("binding proposal")
    };
    assert_eq!(
        proposal
            .remove()
            .unwrap()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["input"]
    );
    let edited = removal
        .document()
        .edit(&[Edit::remove(&["operations", "a", "input"])])
        .unwrap();
    let edited = edited.validate().unwrap().conformant().unwrap();
    let next = complete(client.pull_source(
        PullRequest::new(edited, "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    assert_eq!(
        next.report().existing().unwrap().next().unwrap().status(),
        ExistingStatus::Current
    );
}
