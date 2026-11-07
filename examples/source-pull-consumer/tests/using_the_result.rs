//! The application steps in `src/using_the_result.rs`, against the source-puller
//! interface's "Using the result".
use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_source_pull::{PullLimits, PullRequest, PullResult};
use serde_json::{Value, json};
use source_pull_consumer::using_the_result::{Error, keep_part, take_non_breaking_updates};
use std::{collections::BTreeSet, sync::atomic::AtomicBool};

fn original() -> Value {
    json!({"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}})
}
fn pair(original: Value, result: Value) -> (PullRequest, PullResult) {
    let validation = Document::parse(original.to_string())
        .unwrap()
        .validate()
        .unwrap();
    (
        PullRequest::new(validation.conformant().unwrap(), "s"),
        PullResult::from_json(JsonValue::parse(result.to_string()).unwrap()).unwrap(),
    )
}
fn keep(request: &PullRequest, result: &PullResult, keys: &[&str]) -> Value {
    value(
        &keep_part(
            result,
            request,
            keys,
            &PullLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap(),
    )
}
fn updates(request: &PullRequest, result: &PullResult) -> Result<ConformantDocument, Error> {
    take_non_breaking_updates(
        result,
        request,
        &PullLimits::default(),
        &AtomicBool::new(false),
    )
}
fn value(document: &ConformantDocument) -> Value {
    serde_json::from_slice(document.as_bytes()).unwrap()
}
fn keys(document: &Value, map: &str) -> BTreeSet<String> {
    document[map]
        .as_object()
        .into_iter()
        .flat_map(|entries| entries.keys().cloned())
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
fn a_binding_brings_its_operation_uses_and_transitive_references() {
    let (request, result) = pair(original(), chain());
    let kept = keep(&request, &result, &["getUser.api"]);
    assert_eq!(keys(&kept, "bindings"), names(&["getUser.api"]));
    assert_eq!(keys(&kept, "operations"), names(&["getUser"]));
    assert_eq!(
        keys(&kept, "schemas"),
        names(&["User", "Address", "City", "ApiError", "ErrorCode"])
    );
}

#[test]
fn ids_anchors_and_subtrees_keep_whole_additions() {
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
    let kept = keep(&request, &result, &["b"]);
    assert_eq!(keys(&kept, "operations"), names(&["o", "x"]));
    assert_eq!(
        keys(&kept, "schemas"),
        names(&["A", "B", "Middle", "Sibling", "OtherSlot"])
    );
}

#[test]
fn reached_bindings_bring_their_own_needs_and_cycles_end() {
    let result = json!({"additions":{
        "operations":{"a":{"input":{"$ref":"#/schemas/A"}},"b":{"output":{"$ref":"#/schemas/B"}}},
        "bindings":{"a":{"operation":"a","source":"s"},"b":{"operation":"b","source":"s"}},
        "schemas":{"A":{"$ref":"#/schemas/B"},"B":{"$ref":"#/schemas/A"},"Data":true}
    },"report":{"exhaustive":true,"added":[{"binding":"a","operation":"a","uses":["bindings/b"]},{"binding":"b","operation":"b","uses":["bindings/a","schemas/Data"]}]}});
    let (request, result) = pair(original(), result);
    let kept = keep(&request, &result, &["a"]);
    assert_eq!(keys(&kept, "bindings"), names(&["a", "b"]));
    assert_eq!(keys(&kept, "schemas"), names(&["A", "B", "Data"]));
}

#[test]
fn a_reused_operation_stays_and_references_pass_through_existing_entries() {
    let mut before = original();
    before["operations"] = json!({"old":{"description":"author","input":true,"output":{"$ref":"urn:example:Unreached"}}});
    before["schemas"] = json!({"Existing":{"properties":{"part":{"$ref":"urn:example:New"},"other":{"$ref":"urn:example:Unreached"}}}});
    let result = json!({"additions":{
        "bindings":{"b":{"operation":"old","source":"s"}},
        "schemas":{"Start":{"allOf":[{"$ref":"#/schemas/Existing/properties/part"},{"$ref":"#/operations/old/input"}]},"New":{"$id":"urn:example:New","$ref":"urn:example:End"},"End":{"$id":"urn:example:End"},"Unreached":{"$id":"urn:example:Unreached"}}
    },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"old","reused":true,"uses":["schemas/Start"]}]}});
    let (request, result) = pair(before.clone(), result);
    let kept = keep(&request, &result, &["b"]);
    assert_eq!(kept["operations"], before["operations"]);
    assert_eq!(kept["schemas"]["Existing"], before["schemas"]["Existing"]);
    assert_eq!(
        keys(&kept, "schemas"),
        names(&["Existing", "Start", "New", "End"])
    );
}

#[test]
fn an_unknown_key_is_refused_and_nothing_chosen_changes_nothing() {
    let (request, result) = pair(original(), chain());
    assert_eq!(
        keep(&request, &result, &[]),
        value(request.interface()),
        "an empty selection keeps the original"
    );
    let unknown = keep_part(
        &result,
        &request,
        &["nope"],
        &PullLimits::default(),
        &AtomicBool::new(false),
    );
    assert!(unknown.is_err());
}

fn described() -> Value {
    let mut before = original();
    before["operations"] = json!({
        "getUser":{"description":"author","output":{"$ref":"#/schemas/User"}},
        "listUsers":{"output":{"type":"array"}}
    });
    before["schemas"] = json!({"User":{"type":"object"}});
    before["sources"]["s"]["data"] = json!({"version":1});
    before["bindings"] = json!({
        "getUser.s":{"operation":"getUser","source":"s","data":{"path":"/users/1"},"preference":4},
        "listUsers.s":{"operation":"listUsers","source":"s","data":{"path":"/users"}}
    });
    before
}

#[test]
fn non_breaking_updates_apply_and_the_authors_fields_stay() {
    let result = json!({"additions":{"schemas":{"Label":{"type":"string"}}},"report":{"exhaustive":true,"added":[],"existing":[
        {"binding":"getUser.s","status":"stale","change":"non-breaking","proposal":{"data":{"path":"/v2/users/1"},"note":"not copied"}},
        {"binding":"listUsers.s","status":"stale","change":"breaking","proposal":{"output":{"type":"object"}}},
        {"schema":"User","status":"stale","change":"non-breaking","proposal":{"schema":{"type":"object","properties":{"label":{"$ref":"#/schemas/Label"}}}}}
    ]}});
    let before = described();
    let (request, result) = pair(before.clone(), result);
    let document = value(&updates(&request, &result).unwrap());
    assert_eq!(
        document["bindings"]["getUser.s"],
        json!({"operation":"getUser","source":"s","data":{"path":"/v2/users/1"},"preference":4})
    );
    assert_eq!(document["operations"]["getUser"]["description"], "author");
    assert_eq!(
        document["schemas"]["User"]["properties"]["label"]["$ref"],
        "#/schemas/Label"
    );
    assert_eq!(document["schemas"]["Label"]["type"], "string");
    // The breaking proposal is left alone, and the original is unchanged.
    assert_eq!(document["operations"]["listUsers"]["output"]["type"], "array");
    assert_eq!(value(request.interface()), before);
}

#[test]
fn a_proposal_can_remove_and_set_what_the_contract_allows() {
    let result = json!({"additions":{},"report":{"exhaustive":true,"added":[],"existing":[
        {"binding":"getUser.s","status":"stale","change":"non-breaking","proposal":{"remove":["output","data"],"input":true}},
        {"binding":"listUsers.s","status":"current"},
        {"source":"s","status":"stale","change":"non-breaking","proposal":{"remove":["data"]}}
    ]}});
    let (request, result) = pair(described(), result);
    let document = value(&updates(&request, &result).unwrap());
    assert!(document["operations"]["getUser"].get("output").is_none());
    assert_eq!(document["operations"]["getUser"]["input"], true);
    assert!(document["bindings"]["getUser.s"].get("data").is_none());
    assert!(document["sources"]["s"].get("data").is_none());
}

#[test]
fn two_proposals_writing_one_place_are_refused_even_when_they_agree() {
    for second in [json!({"type":"number"}), json!({"type":"string"})] {
        let mut before = described();
        before["bindings"]["second.s"] = json!({"operation":"getUser","source":"s"});
        let result = json!({"additions":{},"report":{"exhaustive":true,"added":[],"existing":[
            {"binding":"getUser.s","status":"stale","change":"non-breaking","proposal":{"output":{"type":"string"}}},
            {"binding":"second.s","status":"stale","change":"non-breaking","proposal":{"output":second}},
            {"binding":"listUsers.s","status":"current"}
        ]}});
        let (request, result) = pair(before, result);
        assert!(
            updates(&request, &result)
                .unwrap_err()
                .to_string()
                .contains("overlapping-edits")
        );
    }
}
