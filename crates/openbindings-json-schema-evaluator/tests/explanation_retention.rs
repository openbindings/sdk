//! A prepared schema owns its original environment, but no unrelated preparation. Its own binary, so no concurrent test changes the live arena count.
use openbindings::*;
use openbindings_internal_json::backend::live_arenas;
use openbindings_json_schema_evaluator::DefaultEvaluator;

const ITEM: &str = "https://example.test/item";
const OTHER: &str = "https://example.test/other";

fn resources(uri: &str, schema: &str) -> ExternalSchemas {
    ExternalSchemas::new([ExternalSchema {
        uri: uri.into(),
        document: JsonValue::parse(schema).unwrap(),
    }])
    .unwrap()
}
fn proof() -> ConformantDocument {
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
    type Validator = openbindings_json_schema_evaluator::DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        request.environment(limits, control)?;
        DefaultEvaluator::new().prepare(request, limits, control)
    }
}
fn explaining_contract() -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
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
        .prepare(
            &DefaultEvaluator::new(),
            &item,
            &openbindings::PreparationLimits::default(),
        )
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
            .prepare(
                &Projecting,
                &other,
                &openbindings::PreparationLimits::default(),
            )
            .unwrap(),
    );
    contract
}

#[test]
fn explaining_contracts_release_the_original_environment_on_final_drop() {
    // Warm process-lifetime schemas before counting.
    drop(explaining_contract());
    let before = live_arenas();
    let contract = explaining_contract();
    // The original document and reached resource remain owned by the Plan.
    // The unrelated OTHER resource and preparation have been released.
    assert_eq!(live_arenas(), before + 2);
    let value = JsonValue::parse(r#"{"id":0}"#).unwrap();
    let ValueOutcome::Fails { problems, .. } = contract.explain(&value) else {
        panic!("expected failure")
    };
    let location = &problems[0].schema;
    assert_eq!(location.source_uri.as_deref(), Some(ITEM));
    assert_eq!(location.pointer, "/properties/id/minimum");
    drop(value);
    // Explaining builds no additional arena.
    assert_eq!(live_arenas(), before + 2);
    drop(contract);
    assert_eq!(live_arenas(), before);
}
