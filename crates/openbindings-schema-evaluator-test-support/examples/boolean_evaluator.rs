//! A deliberately limited custom evaluator, independent of the default companion.
use openbindings::*;
struct BooleanEvaluator;
// The schema's value and its original location, for explanations.
struct BooleanContract(bool, SchemaOrigin);
impl SchemaEvaluator for BooleanEvaluator {
    type Validator = BooleanContract;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        _: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        control.check()?;
        match request.schema.json().view().as_bool() {
            Some(value) => Ok(BooleanContract(value, request.schema.location())),
            None => Err(PreparationFailure::new(
                PreparationFailureKind::UnsupportedCapability {
                    capability: "boolean-only".into(),
                },
            )),
        }
    }
}
impl SchemaValidator for BooleanContract {
    fn evaluate(&self, _: &JsonValue, detail: Detail, control: &dyn WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueOutcome::NoVerdict { detail };
        }
        if self.0 {
            return ValueOutcome::Satisfies;
        }
        // A cheap check reports the verdict only; an explanation says why.
        let explain = detail == Detail::Explain;
        ValueOutcome::Fails {
            message: "value does not satisfy the schema".into(),
            problems: explain
                .then(|| {
                    // The whole value fails: the instance's root.
                    ValueProblem::new(
                        "".parse().expect("the root pointer"),
                        self.1.clone(),
                        "false-schema",
                        "a false schema permits no values",
                    )
                })
                .into_iter()
                .collect(),
            problems_complete: explain,
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
        .ok_or("operation has no input schema")?
        .prepare(
            &BooleanEvaluator,
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )?;
    let output = operation
        .output()
        .ok_or("operation has no output schema")?
        .prepare(
            &BooleanEvaluator,
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )?;
    assert!(matches!(input.evaluate(&value), ValueOutcome::Satisfies));
    assert!(matches!(
        output.evaluate(&value),
        ValueOutcome::Fails { problems, .. } if problems.is_empty()
    ));
    let ValueOutcome::Fails {
        problems,
        problems_complete: true,
        ..
    } = output.explain(&value)
    else {
        return Err("the false schema was not explained".into());
    };
    assert_eq!(problems.len(), 1);
    // The document holds the schema's text, so it finds the line and column.
    let schema = document
        .locate(&problems[0].schema)
        .ok_or("schema location")?;
    assert_eq!(schema.pointer.as_ref().unwrap(), "/operations/op/output");
    // The value and the document each locate their side on demand. Pointers
    // are source-controlled text: Debug of the string quotes and escapes it.
    let instance = value
        .locate(&problems[0].instance)
        .ok_or("instance location")?;
    println!(
        "{} at {:?}, line {}, column {} (schema {:?}, line {}, column {})",
        problems[0].code,
        problems[0].instance.to_string(),
        instance.line,
        instance.byte_column,
        problems[0].schema.pointer.to_string(),
        schema.line,
        schema.byte_column
    );
    Ok(())
}
