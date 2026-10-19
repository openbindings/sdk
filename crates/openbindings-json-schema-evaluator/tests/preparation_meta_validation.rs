//! The library validates a schema against its meta-schema before compiling it.
//! Natively the meta-schema's own validator is compiled on first use and kept;
//! on Wasm it is compiled for every build. That compilation is never charged to
//! a preparation's compilation work, so both targets charge the same.
//!
//! This file holds one test so that its build is the first in the process.

#[test]
fn meta_validation_is_not_charged_to_compilation_work() {
    use jsonschema::ob_work::compiling;
    // One compiled node needs one unit, whether or not this build also compiles
    // the meta-schema validator, which has dynamic anchors.
    let schema = serde_json::json!({"type": "integer"});
    let built = compiling(1, true, || jsonschema::validator_for(&schema))
        .expect("one unit of compilation work")
        .expect("valid schema");
    assert!(built.is_valid(&serde_json::json!(7)));
    assert!(!built.is_valid(&serde_json::json!("7")));
}
