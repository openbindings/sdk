//! Emit a complete case-ID receipt through a custom evaluator outside core.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_schema_evaluator_test_support::{Options, run, suite};
use std::sync::Arc;

struct OriginalContext(openbindings_json_schema_evaluator::Diagnostics);
impl SchemaEvaluator for OriginalContext {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        let document: &ValidatedDocument = request.document();
        assert_eq!(
            document
                .value()
                .at(&request.entry_location().pointer)
                .unwrap()
                .to_owned()
                .semantic_eq(request.entry()),
            Some(true)
        );
        DefaultEvaluator::new()
            .with_diagnostics(self.0)
            .prepare(request, control)
    }
}
fn main() {
    let report = run(
        Arc::new(OriginalContext(
            if std::env::args().any(|arg| arg == "--upstream") {
                openbindings_json_schema_evaluator::Diagnostics::Upstream
            } else {
                openbindings_json_schema_evaluator::Diagnostics::Coarse
            },
        )),
        &Options::without_unicode_property_matching(),
    );
    let admission: Vec<_> = suite().groups.iter().map(|group| {
        let document = ParsedDocument::parse(&group.document).unwrap();
        let validation = document.validate().unwrap();
        serde_json::json!({"id":group.id, "document":group.document, "resources":group.resources, "report":validation.report()})
    }).collect();
    println!(
        "{}",
        serde_json::json!({"report":report, "admission":admission})
    );
    assert!(report.is_success(), "{:?}", report.configuration_failures);
}
