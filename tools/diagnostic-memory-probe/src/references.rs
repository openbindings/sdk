//! Inspection storage must not retain another copy of each authored reference.
use super::{JsonValue, LIVE, Relaxed, measured};
use openbindings_internal_schema::{ProjectedResource, RawDocument, SchemaSpace, Supplied};
use serde_json::{Value, json};

fn retained(count: usize, width: usize) -> (usize, usize) {
    let uri = format!("https://memory.test/{}", "a".repeat(width));
    let reference = format!("{uri}#/$defs/value");
    let document = RawDocument::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":{
            "allOf":(0..count).map(|_| json!({"$ref":reference})).collect::<Vec<_>>()
        }}}})
        .to_string(),
    )
    .unwrap();
    let target =
        JsonValue::parse(json!({"$id":uri,"$defs":{"value":{"type":"integer"}}}).to_string())
            .unwrap();
    let space = SchemaSpace::new(
        document,
        Supplied::new([ProjectedResource {
            uri,
            document: target,
        }])
        .unwrap(),
    );
    let holder = space
        .document_node(
            &space
                .document
                .value()
                .at("/operations/op/input/allOf/0")
                .unwrap()
                .to_owned(),
        )
        .unwrap();
    // Original source bytes and indexes already exist. Count only the lazy
    // inspection's additional live/peak System requests, retaining the space.
    let before = LIVE.load(Relaxed);
    let (target, peak, _) = measured(|| space.resolve(holder, &reference).unwrap());
    let retained = LIVE.load(Relaxed).saturating_sub(before);
    assert_eq!(
        space
            .node(target.node)
            .value
            .get("type")
            .and_then(|v| v.as_str()),
        Some("integer")
    );
    (retained, peak)
}

pub fn run() -> Vec<Value> {
    // Initialize fixed process caches before comparing input widths.
    retained(1, 16);
    let mut observations = Vec::new();
    for count in [1_000, 5_000] {
        let short = retained(count, 256);
        let long = retained(count, 1024);
        // Routing/results may grow with reference count, but making each URI
        // 768 bytes longer must not retain count * 768 additional bytes. Allow
        // fixed carrier/registry overhead; even one extra full spelling per
        // query would exceed these ceilings by at least an order of magnitude.
        assert!(
            long.0 <= short.0 + 16 * 1024,
            "retained reference text: {count} queries: short {short:?}, long {long:?}"
        );
        assert!(
            long.1 <= short.1 + 64 * 1024,
            "peak reference text: {count} queries: short {short:?}, long {long:?}"
        );
        observations.push(
            json!({"references":count,"short_width":256,"long_width":1024,
            "short_retained_bytes":short.0,"long_retained_bytes":long.0,
            "short_peak_bytes":short.1,"long_peak_bytes":long.1,
            "retained_growth_ceiling_bytes":16*1024,"peak_growth_ceiling_bytes":64*1024}),
        );
    }
    observations
}
