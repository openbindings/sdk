//! A deliberately limited custom evaluator, independent of the default companion.
use openbindings::*;
use std::sync::Arc;
struct BooleanEvaluator;
struct BooleanContract(bool);
impl SchemaEvaluator for BooleanEvaluator {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        match request.entry().view().as_bool() {
            Some(value) => Ok(Arc::new(BooleanContract(value))),
            None => Err(NoVerdict::new(
                NoVerdictReason::UnsupportedCapability,
                "boolean-only",
                "this example supports only boolean schemas",
            )),
        }
    }
}
impl PreparedSchema for BooleanContract {
    fn validate(&self, _: &JsonValue, control: &dyn WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueOutcome::NoVerdict { detail };
        }
        if self.0 {
            ValueOutcome::Satisfies
        } else {
            ValueOutcome::Fails {
                message: "value does not satisfy the contract".into(),
                problems: vec![ValueProblem {
                    details: None,
                    instance_pointer: String::new(),
                    schema_location: None,
                    code: "false-schema".into(),
                    message: "a false schema permits no values".into(),
                }],
                problems_complete: true,
            }
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":true,"output":false}}}"#,
    )?;
    let document = document
        .validate()?
        .conformant()
        .ok_or("document is not conformant")?;
    let operation = document
        .resolve_operation("op")
        .ok_or("operation is missing")?;
    let value = JsonValue::null();
    let input = operation
        .input()
        .ok_or("operation has no input contract")?
        .prepare(&BooleanEvaluator)?;
    let output = operation
        .output()
        .ok_or("operation has no output contract")?
        .prepare(&BooleanEvaluator)?;
    assert!(matches!(input.validate(&value), ValueOutcome::Satisfies));
    assert!(matches!(
        output.validate(&value),
        ValueOutcome::Fails { .. }
    ));
    Ok(())
}
