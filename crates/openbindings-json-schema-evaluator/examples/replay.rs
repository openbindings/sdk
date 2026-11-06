//! Emit a complete case-ID receipt, checking and explaining every value, through
//! a custom evaluator outside core.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator};
use openbindings_schema_evaluator_test_support::{Report, run, suite};

struct OriginalContext;
impl SchemaEvaluator for OriginalContext {
    type Validator = DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        let document = request.schema.document();
        let entry = request.schema.location();
        assert_eq!(
            document
                .value()
                .at_pointer(&entry.pointer)
                .unwrap()
                .to_owned()
                .semantic_eq(&request.schema.json()),
            Some(true)
        );
        DefaultEvaluator::new().prepare(request, limits, control)
    }
}

fn main() {
    eprintln!("Evaluator replay: executing all value cases");
    let report = run(&OriginalContext);
    eprintln!(
        "Evaluator replay: {} value observations; collecting document admission evidence",
        report.observations.len()
    );
    // The receipt is written as JSON text: a report can carry an exact pointer
    // through a member name with an unpaired unit, which no `Value` holds.
    #[derive(serde::Serialize)]
    struct Admission<'a> {
        document: &'a str,
        id: &'a str,
        report: ValidationReport,
        resources: &'a [String],
    }
    #[derive(serde::Serialize)]
    struct Receipt<'a> {
        admission: Vec<Admission<'a>>,
        report: &'a Report,
    }
    let admission = suite()
        .groups
        .iter()
        .map(|group| {
            let document = Document::parse(&group.document).unwrap();
            Admission {
                document: &group.document,
                id: &group.id,
                report: document.validate().unwrap().report().clone(),
                resources: &group.resources,
            }
        })
        .collect();
    let receipt = Receipt {
        admission,
        report: &report,
    };
    eprintln!("Evaluator replay: serializing the complete receipt");
    let serialized = serde_json::to_string(&receipt).unwrap();
    eprintln!(
        "Evaluator replay: writing {} receipt bytes",
        serialized.len()
    );
    println!("{serialized}");
    eprintln!("Evaluator replay: receipt written; checking success");
    assert!(report.is_success(), "{:?}", report.configuration_failures);
}
