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
    let context = document
        .validate()?
        .conformant()
        .ok_or("document is not conformant")?
        .value_contracts(Arc::new(BooleanEvaluator), ResourceSet::default());
    let value = JsonValue::null();
    let ValueContractPreparation::Ready(input) = context.prepare("op", Direction::Input) else {
        panic!("boolean input should prepare")
    };
    let ValueContractPreparation::Ready(output) = context.prepare("op", Direction::Output) else {
        panic!("boolean output should prepare")
    };
    assert!(matches!(input.validate(&value), ValueOutcome::Satisfies));
    assert!(matches!(
        output.validate(&value),
        ValueOutcome::Fails { .. }
    ));
    Ok(())
}
