use openbindings::{ConformantDocument, Document, JsonValue, ReferenceResolution, WorkControl};
use openbindings_source_pull::{
    PullLimits, PullRequest, PullResult, PullValidationError, RelationshipError,
};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};

fn original() -> Value {
    json!({"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}})
}
fn pair(original: Value, result: Value) -> (PullRequest, PullResult) {
    let document = Document::parse(original.to_string())
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    (
        PullRequest::new(document, "s"),
        PullResult::from_json(JsonValue::parse(result.to_string()).unwrap()).unwrap(),
    )
}
fn keep(request: &PullRequest, result: &PullResult, keys: &[&str]) -> ConformantDocument {
    result
        .keep(
            request,
            keys,
            &PullLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
}
fn value(document: &ConformantDocument) -> Value {
    serde_json::from_slice(document.as_bytes()).unwrap()
}
fn keys(document: &Value, section: &str) -> BTreeSet<String> {
    document[section]
        .as_object()
        .into_iter()
        .flat_map(|map| map.keys().cloned())
        .collect()
}
fn names(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).into()).collect()
}
fn chain() -> Value {
    json!({"additions":{
        "operations":{"getUser":{"output":{"$ref":"#/schemas/User"}},"unrelated":{}},
        "bindings":{"getUser.api":{"operation":"getUser","source":"s","data":{"error":"ApiError"}},"unrelated":{"operation":"unrelated","source":"s"}},
        "schemas":{"User":{"$ref":"#/schemas/Address"},"Address":{"$ref":"#/schemas/City"},"City":{"type":"string"},"ApiError":{"$ref":"#/schemas/ErrorCode"},"ErrorCode":true,"Unused":false}
    },"report":{"exhaustive":true,"added":[{"binding":"getUser.api","operation":"getUser","uses":["schemas/ApiError"]},{"binding":"unrelated","operation":"unrelated"}]}})
}

#[test]
fn binding_operation_data_needs_and_transitive_schema_references() {
    let (request, result) = pair(original(), chain());
    // This is the public array call shape, without type annotations or allocation.
    let kept = result
        .keep(
            &request,
            ["getUser.api"],
            &PullLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    let kept = value(&kept);
    assert_eq!(keys(&kept, "bindings"), names(&["getUser.api"]));
    assert_eq!(keys(&kept, "operations"), names(&["getUser"]));
    assert_eq!(
        keys(&kept, "schemas"),
        names(&["User", "Address", "City", "ApiError", "ErrorCode"])
    );
}

#[test]
fn ids_anchors_operation_inputs_and_schema_subtrees_keep_whole_additions() {
    let result = json!({"additions":{
        "operations":{"o":{"input":{"allOf":[{"$ref":"urn:example:A#part"},{"$ref":"#/operations/x/input"}]}},"x":{"input":{"$ref":"#/schemas/Middle/properties/b"},"output":{"$ref":"#/schemas/OtherSlot"}}},
        "bindings":{"b":{"operation":"o","source":"s"}},
        "schemas":{
            "A":{"$id":"urn:example:A","$defs":{"part":{"$anchor":"part","$ref":"urn:example:B"}}},
            "B":{"$id":"urn:example:B","type":"string"},
            "Middle":{"properties":{"b":true,"sibling":{"$ref":"#/schemas/Sibling"}}},
            "Sibling":true,"OtherSlot":true,"Unrelated":false
        }
    },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"o"}]}});
    let (request, result) = pair(original(), result);
    let kept = value(&keep(&request, &result, &["b"]));
    assert_eq!(keys(&kept, "operations"), names(&["o", "x"]));
    assert_eq!(
        keys(&kept, "schemas"),
        names(&["A", "B", "Middle", "Sibling", "OtherSlot"])
    );
}

#[test]
fn reached_bindings_bring_their_operation_and_uses_and_cycles_terminate() {
    let result = json!({"additions":{
        "operations":{"a":{"input":{"$ref":"#/schemas/A"}},"b":{"output":{"$ref":"#/schemas/B"}}},
        "bindings":{"a":{"operation":"a","source":"s"},"b":{"operation":"b","source":"s"}},
        "schemas":{"A":{"$ref":"#/schemas/B"},"B":{"$ref":"#/schemas/A"},"Data":true}
    },"report":{"exhaustive":true,"added":[{"binding":"a","operation":"a","uses":["bindings/b"]},{"binding":"b","operation":"b","uses":["bindings/a","schemas/Data"]}]}});
    let (request, result) = pair(original(), result);
    let kept = keep(&request, &result, &["a"]);
    let full = result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(value(&kept), value(&full));
    assert_eq!(keys(&value(&kept), "bindings"), names(&["a", "b"]));
    assert_eq!(keys(&value(&kept), "schemas"), names(&["A", "B", "Data"]));
}

#[test]
fn reused_operation_and_references_through_existing_subtrees() {
    let mut before = original();
    before["operations"] = json!({"old":{"description":"author","input":true,"output":{"$ref":"urn:example:Unreached"}}});
    before["schemas"] = json!({"Existing":{"properties":{"part/~":{"$ref":"urn:example:New"},"other":{"$ref":"urn:example:Unreached"}}}});
    let result = json!({"additions":{
        "bindings":{"b":{"operation":"old","source":"s"}},
        "schemas":{"Start":{"allOf":[{"$ref":"#/schemas/Existing/properties/part~1~0"},{"$ref":"#/operations/old/input"}]},"New":{"$id":"urn:example:New","$ref":"urn:example:End"},"End":{"$id":"urn:example:End","type":"string"},"Unreached":{"$id":"urn:example:Unreached"}}
    },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"old","reused":true,"uses":["schemas/Start"]}]}});
    let (request, result) = pair(before.clone(), result);
    let kept = value(&keep(&request, &result, &["b"]));
    assert_eq!(kept["operations"], before["operations"]);
    assert_eq!(kept["schemas"]["Existing"], before["schemas"]["Existing"]);
    assert_eq!(
        keys(&kept, "schemas"),
        names(&["Existing", "Start", "New", "End"])
    );
}

#[test]
fn proposals_stay_unapplied_and_their_additions_are_left_out() {
    let mut before = original();
    before["operations"] = json!({"old":{"input":true,"description":"author"}});
    before["bindings"] =
        json!({"old":{"operation":"old","source":"s","data":{"old":true},"preference":4}});
    before["schemas"] = json!({"Old":true});
    let mut result = chain();
    result["additions"]["schemas"]["ProposalOnly"] = json!({"$ref":"#/schemas/Unused"});
    result["report"]["existing"] = json!([
        {"binding":"old","status":"stale","change":"dangerous","proposal":{"data":null,"input":{"$ref":"#/schemas/ProposalOnly"},"uses":["schemas/ProposalOnly"]}},
        {"schema":"Old","status":"stale","change":"dangerous","proposal":{"schema":{"$ref":"#/schemas/ProposalOnly"}}},
        {"source":"s","status":"stale","change":"dangerous","proposal":{"data":{"new":true}}}
    ]);
    let (request, result) = pair(before.clone(), result);
    let kept = value(&keep(&request, &result, &["getUser.api"]));
    assert_eq!(kept["operations"]["old"], before["operations"]["old"]);
    assert_eq!(kept["bindings"]["old"], before["bindings"]["old"]);
    assert_eq!(kept["sources"], before["sources"]);
    assert_eq!(kept["schemas"]["Old"], before["schemas"]["Old"]);
    assert!(kept["schemas"].get("ProposalOnly").is_none());
    assert!(kept["schemas"].get("Unused").is_none());
}

#[test]
fn selection_keys_empty_repeated_and_original_bytes() {
    let mut before = original();
    before["operations"] = json!({"old":{}});
    before["bindings"] = json!({"old":{"operation":"old","source":"s"}});
    let mut supplied = chain();
    supplied["report"]["existing"] = json!([{"binding":"old","status":"current"}]);
    let (request, result) = pair(before, supplied);
    let bytes = request.interface().as_bytes().to_vec();
    for key in ["missing", "old"] {
        assert!(matches!(
            result.keep(
                &request,
                [key],
                &PullLimits::default(),
                &AtomicBool::new(false)
            ),
            Err(PullValidationError::Relationship(
                RelationshipError::KeptBinding
            ))
        ));
    }
    assert_eq!(keep(&request, &result, &[]).as_bytes(), bytes);
    assert_eq!(
        value(&keep(&request, &result, &["getUser.api", "getUser.api"])),
        value(&keep(&request, &result, &["getUser.api"]))
    );
    assert_eq!(request.interface().as_bytes(), bytes);
}

#[test]
fn validation_refusals_are_preserved_even_for_empty_selection() {
    for fault in 0..3 {
        let mut supplied = chain();
        match fault {
            0 => supplied["additions"]["bindings"]["getUser.api"]["source"] = json!("absent"),
            1 => supplied["report"]["added"][0]["uses"] = json!(["schemas/Missing"]),
            _ => {
                supplied["additions"]["operations"]["getUser"]["input"] =
                    json!({"$ref":"#/schemas/Missing"})
            }
        }
        let (request, result) = pair(original(), supplied);
        let expected = result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap_err();
        for selected in [vec![], vec!["getUser.api"], vec!["unknown"]] {
            let actual = result
                .keep(
                    &request,
                    selected,
                    &PullLimits::default(),
                    &AtomicBool::new(false),
                )
                .unwrap_err();
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        }
    }
}

struct Polls {
    calls: AtomicUsize,
    cancel_at: usize,
}
impl WorkControl for Polls {
    fn is_cancelled(&self) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed) >= self.cancel_at
    }
}
#[test]
fn cancellation_and_one_shared_work_and_allocation_ledger() {
    let (request, result) = pair(original(), chain());
    let control = Polls {
        calls: AtomicUsize::new(0),
        cancel_at: usize::MAX,
    };
    result
        .keep(&request, ["getUser.api"], &PullLimits::default(), &control)
        .unwrap();
    for cancel_at in 0..control.calls.load(Ordering::Relaxed) {
        let control = Polls {
            calls: AtomicUsize::new(0),
            cancel_at,
        };
        assert!(
            matches!(
                result.keep(&request, ["getUser.api"], &PullLimits::default(), &control),
                Err(PullValidationError::Cancelled)
            ),
            "checkpoint {cancel_at}"
        );
    }
    for allocation in [false, true] {
        // Find validate_for's exact allowance, then prove keep shares that ledger
        // instead of resetting it before building or walking the graph.
        let mut low = 0;
        let mut high = 1_000_000;
        while low < high {
            let middle = low + (high - low) / 2;
            let mut limits = PullLimits::default();
            if allocation {
                limits.max_relationship_allocation_bytes = middle;
            } else {
                limits.max_relationship_work = middle;
            }
            if result
                .validate_for(&request, &limits, &AtomicBool::new(false))
                .is_ok()
            {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let mut limits = PullLimits::default();
        if allocation {
            limits.max_relationship_allocation_bytes = low;
        } else {
            limits.max_relationship_work = low;
        }
        result
            .validate_for(&request, &limits, &AtomicBool::new(false))
            .unwrap();
        let expected = if allocation {
            "relationship-allocation-bytes"
        } else {
            "relationship-work"
        };
        assert!(
            matches!(result.keep(&request, ["getUser.api"], &limits, &AtomicBool::new(false)), Err(PullValidationError::LimitExceeded(resource)) if resource == expected)
        );
    }
    keep(&request, &result, &["getUser.api"]);
}

#[test]
fn every_binding_without_proposal_only_additions_matches_full_validation() {
    let mut supplied = chain();
    supplied["additions"]["schemas"]
        .as_object_mut()
        .unwrap()
        .remove("Unused");
    for present_maps in [false, true] {
        let mut before = original();
        if present_maps {
            before["schemas"] = json!({});
            before["bindings"] = json!({});
        }
        let (request, result) = pair(before, supplied.clone());
        let kept = keep(&request, &result, &["getUser.api", "unrelated"]);
        let full = result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(value(&kept), value(&full));
    }
}

#[test]
fn published_cases_keep_each_binding_and_all_without_losing_resolved_references() {
    let pack: Value = serde_json::from_str(include_str!("fixtures/pull-report.json")).unwrap();
    let mut cases = 0;
    let mut selections = 0;
    for case in pack["tests"].as_array().unwrap() {
        if case["schemaValid"] != true || case["report"]["added"].as_array().unwrap().is_empty() {
            continue;
        }
        let original = Document::parse(case["interface"].to_string())
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        let request = PullRequest::new(original, case["source"].as_str().unwrap());
        let result = PullResult::from_json(
            JsonValue::parse(
                json!({"additions":case["additions"],"report":case["report"]}).to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        let full = result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap();
        let full_refs = full.document().references();
        assert!(full_refs.complete);
        let bindings: Vec<_> = case["report"]["added"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["binding"].as_str().unwrap())
            .collect();
        for chosen in bindings
            .iter()
            .map(|binding| vec![*binding])
            .chain(std::iter::once(bindings.clone()))
        {
            let kept = result
                .keep(
                    &request,
                    chosen,
                    &PullLimits::default(),
                    &AtomicBool::new(false),
                )
                .unwrap_or_else(|error| panic!("{}: {error:?}", case["description"]));
            let kept_refs = kept.document().references();
            assert!(kept_refs.complete);
            for reference in kept_refs.references {
                let resolved_before = full_refs.references.iter().any(|full| full.location == reference.location && matches!(&full.resolution, ReferenceResolution::Located {target, ..} if target.source_uri.is_none()));
                if resolved_before {
                    assert!(
                        matches!(reference.resolution, ReferenceResolution::Located {target, ..} if target.source_uri.is_none()),
                        "{}: {:?}",
                        case["description"],
                        reference.location
                    );
                }
            }
            selections += 1;
        }
        cases += 1;
    }
    eprintln!("published keep cases={cases}, selections={selections}");
    assert_eq!(cases, 8);
    assert_eq!(selections, 16);
}

fn scale(count: usize, filler: usize) -> (PullRequest, PullResult) {
    let operations: Map<_, _> = (0..count).map(|i| (format!("o{i}"), json!({"input":{"allOf":[{"$ref":"#/schemas/A"},{"$ref":"#/schemas/B"},{"$ref":"#/schemas/C"}]}}))).collect();
    let bindings: Map<_, _> = (0..count)
        .map(|i| {
            (
                format!("b{i}"),
                json!({"operation":format!("o{i}"),"source":"s"}),
            )
        })
        .collect();
    let added: Vec<_> = (0..count)
        .map(|i| json!({"binding":format!("b{i}"),"operation":format!("o{i}")}))
        .collect();
    pair(
        original(),
        json!({"additions":{"operations":operations,"bindings":bindings,"schemas":{"A":{"description":"x".repeat(filler),"$ref":"#/schemas/B"},"B":{"$ref":"#/schemas/C"},"C":{"$ref":"#/schemas/A"}}},"report":{"exhaustive":true,"added":added}}),
    )
}
fn measure_scale(request: PullRequest, result: PullResult, count: usize, label: &str) {
    let before = request.interface().as_bytes().to_vec();
    for all in [false, true] {
        let selected: Vec<_> = (0..if all { count } else { 1 })
            .map(|i| format!("b{i}"))
            .collect();
        let start = Instant::now();
        let kept = result
            .keep(
                &request,
                &selected,
                &PullLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        eprintln!(
            "keep scale {label}, result_bytes={}, selection={}, elapsed={:.6}s",
            result.value().value().text().len(),
            if all { "all" } else { "one" },
            start.elapsed().as_secs_f64()
        );
        assert_eq!(kept.operations().len(), selected.len());
        assert_eq!(
            kept.value().get("bindings").unwrap().len(),
            Some(selected.len())
        );
        assert_eq!(kept.value().get("schemas").unwrap().len(), Some(3));
    }
    assert_eq!(request.interface().as_bytes(), before);
}
#[test]
fn scale_5000_operations() {
    let (request, result) = scale(5000, 0);
    measure_scale(request, result, 5000, "5000 operations");
}
#[test]
fn scale_near_30_mib() {
    let (request, result) = scale(2, 30 * 1024 * 1024);
    measure_scale(request, result, 2, "30 MiB");
}

#[test]
fn incomplete_reference_inspection_is_a_limit_failure() {
    let mut schema = json!({"$ref":"urn:example:external"});
    for _ in 0..258 {
        schema = json!({"allOf":[schema]});
    }
    let mut supplied = chain();
    supplied["additions"]["schemas"]["Deep"] = schema;
    let (request, result) = pair(original(), supplied);
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    assert!(matches!(
        result.keep(
            &request,
            ["getUser.api"],
            &PullLimits::default(),
            &AtomicBool::new(false)
        ),
        Err(PullValidationError::LimitExceeded("reference-inspection"))
    ));
}

#[test]
fn keeping_every_addition_preserves_explicit_empty_maps_but_empty_selection_does_not_edit() {
    let supplied = json!({"additions":{
        "operations":{"o":{}},"schemas":{},"bindings":{"b":{"operation":"o","source":"s"}}
    },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"o"}]}});
    let (request, result) = pair(original(), supplied);
    let full = result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(value(&keep(&request, &result, &["b"])), value(&full));
    assert_eq!(
        keep(&request, &result, &[]).as_bytes(),
        request.interface().as_bytes()
    );
    let (request, result) = pair(
        original(),
        json!({"additions":{"schemas":{},"bindings":{}},"report":{"exhaustive":true,"added":[]}}),
    );
    assert_eq!(
        keep(&request, &result, &[]).as_bytes(),
        request.interface().as_bytes()
    );
}
