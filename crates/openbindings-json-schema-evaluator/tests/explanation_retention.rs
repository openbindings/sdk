//! A prepared contract that can explain keeps only what its own explanations
//! read. Its own binary, so no concurrent test changes the live arena count.
use openbindings::*;
use openbindings_internal_json::backend::live_arenas;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

const ITEM: &str = "https://example.test/item";
const OTHER: &str = "https://example.test/other";

fn resources(uri: &str, schema: &str) -> ResourceSet {
    ResourceSet::new([SchemaResource {
        uri: uri.into(),
        document: JsonValue::parse(schema).unwrap(),
    }])
    .unwrap()
}
fn proof() -> Conformant<Document> {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$ref":"{ITEM}"}}}},"other":{{"input":{{"$ref":"{OTHER}"}}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
}
// An evaluator that asks for the core projection, so the document's revision
// assembles a resolution space for the resources it is given.
struct Projecting;
impl SchemaEvaluator for Projecting {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        request.evaluation_program(control)?;
        DefaultEvaluator::new().prepare(request, control)
    }
}
fn explaining_contract() -> PreparedContract {
    let proof = proof();
    let item = resources(
        ITEM,
        r#"{"type":"object","properties":{"id":{"type":"integer","minimum":1}},"required":["id"]}"#,
    );
    let contract = proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(&DefaultEvaluator::new(), &item)
        .unwrap();
    // Another set prepared on the same revision, with a projecting evaluator,
    // must not stay alive through this contract.
    let other = resources(
        OTHER,
        &format!(r#"{{"enum":[{}]}}"#, "1,".repeat(999) + "1"),
    );
    let operation = proof.resolve_operation("other").unwrap();
    drop(
        operation
            .input()
            .unwrap()
            .prepare_with(&Projecting, &other)
            .unwrap(),
    );
    contract
}

#[test]
fn explaining_contracts_keep_neither_documents_nor_resources() {
    // Warm process-lifetime schemas before counting.
    drop(explaining_contract());
    let before = live_arenas();
    let contract = explaining_contract();
    // The document, both resource sets, the views and the resolution space are
    // released; the contract holds no arena of theirs.
    assert_eq!(live_arenas(), before);
    let value = JsonValue::parse(r#"{"id":0}"#).unwrap();
    let ValueOutcome::Fails { problems, .. } = contract.explain(&value) else {
        panic!("expected failure")
    };
    let location = problems[0].schema_location.as_ref().unwrap();
    assert_eq!(location.resource.as_deref(), Some(ITEM));
    assert_eq!(location.pointer, "/properties/id/minimum");
    assert!(matches!(
        &problems[0].details,
        Some(ValueProblemDetails::NumericBound { bound }) if bound == "1"
    ));
    drop(value);
    // Explaining builds no arena either.
    assert_eq!(live_arenas(), before);
    drop(contract);
    assert_eq!(live_arenas(), before);
}
