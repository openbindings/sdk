//! Cold fixed-validator construction runs in its own test executable so other
//! tests cannot warm its process-wide immutable caches during measurement.
use openbindings::*;
use openbindings_allocation_test_counter::{Meter, measure};
use openbindings_internal_schema::preparation::audit;
use openbindings_json_schema_evaluator::DefaultEvaluator;
#[global_allocator]
static METER: Meter = Meter;
#[test]
fn cold_fixed_validators_fit_the_preparation_ledger() {
    for dialect in [
        "https://json-schema.org/draft/2020-12/schema",
        "https://json-schema.org/draft/2019-09/schema",
        "http://json-schema.org/draft-07/schema#",
    ] {
        let source = serde_json::json!({"$schema":dialect,"type":"object","properties":{"id":{"type":"string"},"n":{"type":"integer","minimum":0}},"required":["id"]});
        let resources = ExternalSchemas::new([ExternalSchema {
            uri: "https://perf.test/root".into(),
            document: JsonValue::parse(source.to_string()).unwrap(),
        }])
        .unwrap();
        audit::LAST_PEAK.set(0);
        let (result, sample) = measure(|| {
            resources.prepare_with_control(
                "https://perf.test/root",
                &DefaultEvaluator::new(),
                &PreparationLimits::default(),
                &std::sync::atomic::AtomicBool::new(false),
            )
        });
        assert!(result.is_ok());
        let ledger = audit::LAST_PEAK.get();
        eprintln!(
            "cold dialect={dialect} ledger={ledger} heap={} ratio={:.6}",
            sample.peak,
            ledger as f64 / sample.peak as f64
        );
        assert!(ledger >= sample.peak, "cold fixed compiler is undercounted");
        assert!(
            ledger <= sample.peak * 2,
            "cold fixed compiler is overcounted"
        );
    }
}
