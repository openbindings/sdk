use openbindings::*;
use serde::Serialize;
use serde_json::{Value, json};

fn serialized(value: &impl Serialize) -> Value {
    let tree = serde_json::to_value(value).unwrap();
    assert_eq!(json!({"value": value})["value"], tree);
    assert_eq!(
        serde_json::from_str::<Value>(&serde_json::to_string(value).unwrap()).unwrap(),
        tree
    );
    let mut bytes = Vec::new();
    serde_json::to_writer(&mut bytes, value).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), tree);
    tree
}

#[test]
fn pointer_diagnostics_are_safe_in_value_and_text_serializers() {
    let pointer = Pointer::from_utf16(&[47, 0xd800, 0xdc00, 0xdc00, 0xd800]).unwrap();
    let lossy = String::from_utf16_lossy(&pointer.utf16_units().collect::<Vec<_>>());
    let origin = SchemaOrigin::new(None, pointer.clone());
    assert_eq!(serialized(&origin)["pointer"], lossy);
    let problem = ValueProblem::new(pointer.clone(), origin.clone(), "type", "wrong type");
    assert_eq!(serialized(&problem)["instance"], lossy);
    assert_eq!(serialized(&problem)["schema"]["pointer"], lossy);
    let failure =
        PreparationFailure::new(PreparationFailureKind::Cancelled).with_origin(origin.clone());
    assert_eq!(serialized(&failure)["origin"]["pointer"], lossy);
    for outcome in [
        ValueOutcome::Fails {
            message: "wrong type".into(),
            problems: vec![problem],
            problems_complete: false,
        },
        ValueOutcome::Undefined {
            detail: UndefinedResult::new("undefined", "unknown").with_location(origin.clone()),
        },
        ValueOutcome::NoVerdict {
            detail: NoVerdict::new(NoVerdictReason::ResourceUnavailable, "missing", "unknown")
                .with_location(origin),
        },
    ] {
        serialized(&outcome);
    }
    assert_eq!(
        pointer.utf16_units().collect::<Vec<_>>(),
        [47, 0xd800, 0xdc00, 0xdc00, 0xd800]
    );
}

#[test]
fn findings_locations_and_references_replace_units_without_changing_stored_pointers() {
    let draft = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"properties":{"\ud800":{"$ref":"https://missing.test/s"}}}}}}"#).unwrap();
    let validation = draft.validate().unwrap();
    let finding = validation
        .report()
        .findings
        .iter()
        .find(|f| f.code == "unpaired-surrogate")
        .unwrap();
    let location = finding.location.as_ref().unwrap();
    assert_eq!(
        serialized(finding)["location"]["pointer"],
        "/operations/op/input/properties/�"
    );
    assert_eq!(
        serialized(location)["pointer"],
        "/operations/op/input/properties/�"
    );
    serialized(validation.report());
    let exact = location.pointer.as_ref().unwrap();
    assert!(exact.as_str().is_none());
    assert!(draft.value().at_pointer(exact).is_some());
    let report = draft.references();
    let reference = &report.references[0];
    assert_eq!(
        serialized(reference)["location"]["pointer"],
        "/operations/op/input/properties/�/$ref"
    );
    let ReferenceResolution::Unresolved { detail } = &reference.resolution else {
        panic!("missing resource")
    };
    assert_eq!(
        serialized(detail)["location"]["pointer"],
        "/operations/op/input/properties/�"
    );
    serialized(&report);
    assert!(reference.location.pointer.as_str().is_none());
    assert!(
        draft
            .value()
            .at_pointer(&reference.location.pointer)
            .is_some()
    );
}
