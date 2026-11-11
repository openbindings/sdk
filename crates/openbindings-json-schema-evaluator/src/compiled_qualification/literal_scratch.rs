//! Cancellation inside the integral composite scan.
use super::*;

#[test]
fn integral_composite_scan_honours_cancellation() {
    let (selected, _) = owners(
        r#"{"items":{"type":"integer","minimum":0}}"#,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    struct CancelAt(AtomicUsize);
    impl WorkControl for CancelAt {
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 20
        }
    }
    // Only a value over the estimate reaches the scan.
    let value = JsonValue::parse(format!("[{}0]", "0,".repeat(60_000))).unwrap();
    compiled::entries::take();
    let result = selected
        .compiled
        .outcome(&value, &CancelAt(AtomicUsize::new(0)));
    assert!(
        matches!(result, Some(ValueOutcome::NoVerdict { detail }) if detail.reason == NoVerdictReason::Cancelled)
    );
}
