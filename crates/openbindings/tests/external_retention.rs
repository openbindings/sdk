use openbindings::*;
use openbindings_internal_json::backend;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn resource(uri: usize, document: JsonValue) -> ExternalSchema {
    ExternalSchema {
        uri: format!("https://retained.test/{uri}"),
        document,
    }
}
fn array() -> String {
    format!("{{\"const\":[{}]}}", vec!["0"; 16_384].join(","))
}
fn exhausted(error: ExternalSchemaError) {
    assert!(
        matches!(error.kind, ExternalSchemaErrorKind::Limit { ref resource }
        if resource == "preparation-sdk-reservation-bytes")
    );
}
#[test]
fn root_owners_are_charged_once_and_equal_independent_roots_are_distinct() {
    let text = array();
    let root = JsonValue::parse(&text).unwrap();
    let limits = PreparationLimits {
        max_sdk_reservation_bytes: backend::retained_reservation(&root) + 256 * 1024,
        ..Default::default()
    };
    let shared = ExternalSchemas::new_with_limits(
        (0..64).map(|i| resource(i, root.clone())),
        &limits,
        &AtomicBool::new(false),
    );
    assert!(shared.is_ok(), "shared roots must not be charged 64 times");
    exhausted(
        ExternalSchemas::new_with_limits(
            (0..2).map(|i| resource(i, JsonValue::parse(&text).unwrap())),
            &limits,
            &AtomicBool::new(false),
        )
        .unwrap_err(),
    );
}
#[test]
fn default_reservation_stops_a_lazy_stream_before_the_443rd_root_is_kept() {
    let text = array();
    let yielded = AtomicUsize::new(0);
    let result = ExternalSchemas::new((0..729).map(|i| {
        yielded.fetch_add(1, Ordering::Relaxed);
        resource(i, JsonValue::parse(&text).unwrap())
    }));
    exhausted(result.unwrap_err());
    // Iterator::next allocates the rejected item before registration can check it.
    // The 443rd owner may be yielded, but it must never enter the retained set.
    assert!(yielded.load(Ordering::Relaxed) <= 443);
}
#[test]
fn selected_subtree_is_detached_and_charged_as_its_new_owner() {
    let text = format!(
        "{{\"large\":\"{}\",\"schema\":{{}}}}",
        "x".repeat(1_000_000)
    );
    let root = JsonValue::parse(text).unwrap();
    let selected = root.get("schema").unwrap().to_owned();
    let limits = PreparationLimits {
        max_sdk_reservation_bytes: 32_768,
        ..Default::default()
    };
    assert!(
        ExternalSchemas::new_with_limits([resource(0, selected)], &limits, &AtomicBool::new(false))
            .is_ok()
    );
    exhausted(
        ExternalSchemas::new_with_limits([resource(0, root)], &limits, &AtomicBool::new(false))
            .unwrap_err(),
    );
}
