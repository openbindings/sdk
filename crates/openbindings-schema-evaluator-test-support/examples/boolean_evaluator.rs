//! A deliberately limited custom evaluator, independent of the default companion.
use openbindings::*;
use std::sync::Arc;
struct BooleanEvaluator;
struct BooleanContract(bool);
impl SchemaEvaluator for BooleanEvaluator {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
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
    fn validate(&self, _: &JsonValue, control: &WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
            return ValueOutcome::NoVerdict { detail };
        }
        if self.0 {
            ValueOutcome::Satisfies
        } else {
            ValueOutcome::Mismatch {
                problems: vec![ValueProblem {
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
    let document = ParsedDocument::parse(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":true,"output":false}}}"#,
    )?;
    let context = document.value_contracts(Arc::new(BooleanEvaluator), ResourceSet::default())?;
    let value = JsonValue::null();
    let ContractPreparation::Ready(input) = context.prepare("op", Side::Input) else {
        panic!("boolean input should prepare")
    };
    let ContractPreparation::Ready(output) = context.prepare("op", Side::Output) else {
        panic!("boolean output should prepare")
    };
    assert!(matches!(input.validate(&value), ValueOutcome::Satisfies));
    assert!(matches!(
        output.validate(&value),
        ValueOutcome::Mismatch { .. }
    ));
    Ok(())
}
