//! Emit a complete case-ID receipt, checking and explaining every value, through
//! a custom evaluator outside core.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_schema_evaluator_test_support::{Options, Report, run, suite};
use std::sync::Arc;

struct OriginalContext;
impl SchemaEvaluator for OriginalContext {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        let document: &Conformant<Document> = request.document();
        let entry = document.locate(&request.entry_location()).unwrap();
        assert_eq!(
            document
                .value()
                .at(entry.pointer.as_ref().and_then(Pointer::as_str).unwrap())
                .unwrap()
                .to_owned()
                .semantic_eq(request.entry()),
            Some(true)
        );
        DefaultEvaluator::new().prepare(request, control)
    }
}
fn main() {
    // The default evaluator's qualified limitations.
    let report = run(
        &OriginalContext,
        &Options::without(&[
            "unicode-property-matching",
            "backtracking-pattern-matching",
            "undefined-results",
        ]),
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
    println!("{}", serde_json::to_string(&receipt).unwrap());
    assert!(report.is_success(), "{:?}", report.configuration_failures);
}
