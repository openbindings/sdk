use openbindings::{Pointer, SchemaOrigin, ValueOutcome, ValueProblem};
use openbindings_schema_evaluator_test_support::judge_interruptions;

fn explanation(unit: u16, schema_pointer: bool) -> ValueOutcome {
    let pointer = Pointer::from_utf16(&[b'/' as u16, unit]).unwrap();
    let (instance, schema) = if schema_pointer {
        ("".parse().unwrap(), pointer)
    } else {
        (pointer, "".parse().unwrap())
    };
    ValueOutcome::Fails {
        message: "value failed".into(),
        problems: vec![ValueProblem::new(
            instance,
            SchemaOrigin::new(None, schema),
            "type",
            "wrong type",
        )],
        problems_complete: true,
    }
}

#[test]
fn interruption_judgment_distinguishes_unpaired_units_and_replacement_characters() {
    for schema_pointer in [false, true] {
        for first in [0xd800, 0xd801, 0xdc00, 0xfffd] {
            for last in [0xd800, 0xd801, 0xdc00, 0xfffd] {
                let uninterrupted = explanation(first, schema_pointer);
                let interrupted = explanation(last, schema_pointer);
                // Ordinary serialization is intentionally lossy for exact pointers.
                assert_eq!(
                    serde_json::to_string(&uninterrupted).unwrap(),
                    serde_json::to_string(&interrupted).unwrap()
                );
                let failures = judge_interruptions(&uninterrupted, &[(1, interrupted)]);
                assert_eq!(
                    failures.is_empty(),
                    first == last,
                    "units {first:x}/{last:x}, schema pointer {schema_pointer}"
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
struct NestedMismatch(u16);
impl openbindings::SchemaEvaluator for NestedMismatch {
    type Validator = Self;
    fn prepare(
        &self,
        _: openbindings::EvaluationRequest<'_>,
        _: &openbindings::PreparationLimits,
        _: &dyn openbindings::WorkControl,
    ) -> Result<Self, openbindings::PreparationFailure> {
        Ok(*self)
    }
}
impl openbindings::SchemaValidator for NestedMismatch {
    fn evaluate(
        &self,
        value: &openbindings::JsonValue,
        detail: openbindings::Detail,
        _: &dyn openbindings::WorkControl,
    ) -> ValueOutcome {
        if detail == openbindings::Detail::Outcome {
            return ValueOutcome::Fails {
                message: "value failed".into(),
                problems: vec![],
                problems_complete: false,
            };
        }
        let offset = value.locate(&"".parse().unwrap()).unwrap().byte_offset;
        explanation(if offset == 0 { 0xd800 } else { self.0 }, false)
    }
}

#[test]
fn nested_value_judgment_compares_exact_pointers() {
    for changed_unit in [0xd801, 0xfffd] {
        let report = openbindings_schema_evaluator_test_support::run(&NestedMismatch(changed_unit));
        assert!(report.observations.iter().any(|observation| {
            observation.failures.iter().any(|failure| {
                failure == "an explanation of the value within a larger text differs"
            })
        }));
    }
}
