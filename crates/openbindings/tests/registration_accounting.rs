//! Registration uses the same requested-live-heap qualification as preparation.
use openbindings::*;
use openbindings_allocation_test_counter::{Meter, measure};
use openbindings_internal_schema::preparation::audit;
use std::sync::atomic::AtomicBool;

#[global_allocator]
static METER: Meter = Meter;

fn records(n: usize) -> String {
    // Exact field order and record shape from tools/workers-memory/fixtures.mjs.
    let record = r#"{"type":"object","properties":{"id":{"type":"integer"},"name":{"type":"string","maxLength":64},"price":{"type":"number","minimum":0},"active":{"type":"boolean"},"tags":{"type":"array","items":{"type":"string"}},"kind":{"enum":["a","b","c"]},"note":{"type":"string"},"count":{"type":"integer","minimum":0}},"required":["id","name"],"additionalProperties":false}"#;
    format!(
        "{{\"type\":\"object\",\"properties\":{{{}}}}}",
        (0..n)
            .map(|i| format!("\"r{i}\":{record}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn register(
    value: &JsonValue,
    count: usize,
    bytes: usize,
) -> Result<ExternalSchemas, ExternalSchemaError> {
    ExternalSchemas::new_with_limits(
        (0..count).map(|i| ExternalSchema {
            uri: format!("https://records.test/schema/{i}"),
            document: value.clone(),
        }),
        &PreparationLimits {
            max_sdk_reservation_bytes: bytes,
            ..Default::default()
        },
        &AtomicBool::new(false),
    )
}
fn limit(error: ExternalSchemaError) {
    assert!(
        matches!(error.kind, ExternalSchemaErrorKind::Limit { resource }
        if resource == "preparation-sdk-reservation-bytes")
    );
}

#[test]
fn supplied_records_register_with_held_storage_allowances_and_retry_after_refusal() {
    for (n, mib) in [(1000, 4), (3000, 16)] {
        let value = JsonValue::parse(records(n)).unwrap();
        limit(register(&value, 1, 1024).unwrap_err());
        let resources = register(&value, 1, mib * 1048576).unwrap();
        assert_eq!(resources.iter().count(), 1);
        assert_eq!(
            resources.iter().next().unwrap().document.bytes(),
            value.bytes()
        );
    }
}

fn qualify(source: &str, count: usize, subtree: bool) {
    let (root, parsed) = measure(|| JsonValue::parse(source).unwrap());
    let value = if subtree {
        root.get("schema").unwrap().to_owned()
    } else {
        root.clone()
    };
    // Registration retains the supplied root once. A selected subtree instead
    // creates a detached arena inside the measured registration interval.
    let held = if subtree { 0 } else { parsed.retained as usize };
    audit::LAST_PEAK.set(0);
    let (resources, sample) = measure(|| register(&value, count, 512 * 1048576).unwrap());
    let charged = audit::LAST_PEAK.get();
    let peak = held + sample.peak;
    eprintln!(
        "registration source={} count={count} subtree={subtree} charge={charged} heap={peak} ratio={:.6}",
        source.len(),
        charged as f64 / peak as f64
    );
    assert!(
        charged >= peak && charged <= peak * 17 / 10,
        "registration charge {charged} must cover held heap {peak} within a factor of 1.7"
    );
    drop(resources);
    // Confirm the public allowance, including the byte immediately below it.
    limit(register(&value, count, charged - 1).unwrap_err());
    assert!(register(&value, count, charged).is_ok());
}

#[test]
fn registration_ledger_tracks_collection_capacities_and_shared_owners() {
    for n in [1, 30, 300, 1000, 3000] {
        qualify(&records(n), 1, false);
    }
    for source in ["true", "{}", r#"{"type":"integer"}"#] {
        for count in [1, 2, 11, 12, 64, 300] {
            qualify(source, count, false);
        }
    }
    qualify(
        &format!("{{\"const\":\"{}\"}}", "x".repeat(262144)),
        1,
        false,
    );
    qualify(
        &format!("{{\"large\":\"{}\",\"schema\":{{}}}}", "x".repeat(1000000)),
        1,
        true,
    );
    qualify(&format!("{{\"schema\":{}}}", records(300)), 1, true);
    for schema in [
        format!("{{\"const\":\"{}\"}}", "x".repeat(262144)),
        format!("{{\"const\":[{}]}}", vec!["0"; 16384].join(",")),
        format!("{{\"const\":{}0{}}}", "[".repeat(1000), "]".repeat(1000)),
        format!("{{\"const\":\"{}\\ud800\"}}", "x".repeat(10000)),
        format!("{{\"const\":\"{}\"}}", "\\u0061".repeat(10000)),
        format!(
            "{{\"const\":{{{}}}}}",
            (0..1000)
                .map(|i| format!("\"k{i}\":0"))
                .collect::<Vec<_>>()
                .join(",")
        ),
    ] {
        qualify(&format!("{{\"schema\":{schema}}}"), 1, true);
    }
    for n in [1000, 4000, 16000, 49000] {
        let defs = (0..n)
            .map(|i| format!(r##""d{i}":{{"$ref":"#/$defs/d{}"}}"##, i + 1))
            .collect::<Vec<_>>()
            .join(",");
        qualify(
            &format!(r##"{{"$defs":{{{defs},"d{n}":{{"type":"integer"}}}},"$ref":"#/$defs/d0"}}"##),
            1,
            false,
        );
    }
    for n in [30, 300, 1000] {
        let ids = (0..n)
            .map(|i| format!(r#"{{"$id":"child/{i}","$anchor":"a{i}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        qualify(&format!("{{\"allOf\":[{ids}]}}"), 1, false);
    }
}
