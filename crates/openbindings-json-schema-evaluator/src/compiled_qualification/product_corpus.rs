//! Opt-in real corpus measurement through the public preparation/evaluation path.
use super::*;
use openbindings_internal_json::backend;
use std::time::Instant;

fn measure(validator: &DefaultValidator, value: &JsonValue) -> serde_json::Value {
    compiled::entries::take();
    let start = Instant::now();
    let outcome = validator.evaluate(value, Detail::Outcome, &HEALTHY);
    let elapsed = start.elapsed().as_micros();
    let route = if compiled::entries::take()[2] == 1 {
        "compiled"
    } else {
        "original"
    };
    json!({"route":route,"evaluation_us":elapsed,"outcome":serialized(outcome)})
}

fn populated(job: &serde_json::Value, count: usize) -> serde_json::Value {
    let page = json!({"uniqueId":"x","title":"x","language":"nl","permissions":{},
        "layout":{"rows":[{"widgets":vec![json!({"type":"text","content":"x"});count]}]}});
    let id = job["id"].as_str().unwrap();
    if id.starts_with("obi-source-puller") {
        // Each entry is a real schema-governed report item, with an opaque
        // proposal large enough to exercise the parser's full node allowance.
        json!({"additions":{},"report":{"exhaustive":true,"added":[],"existing":vec![json!({"binding":"x","status":"current"});count]}})
    } else if id.contains("from-template") {
        json!({"success":true,"page":page})
    } else if id.contains("templates") {
        json!({"template":page})
    } else if id.ends_with("/PageLayout") {
        page["layout"].clone()
    } else {
        page
    }
}

fn sized(job: &serde_json::Value, bytes: usize, dense: bool) -> JsonValue {
    let count = if dense { (bytes / 40).min(190_000) } else { 1 };
    let mut value = populated(job, count);
    value["padding"] = json!("");
    let length = value.to_string().len();
    assert!(length <= bytes, "{length} > {bytes}");
    value["padding"] = json!("x".repeat(bytes - length));
    let text = value.to_string();
    assert_eq!(text.len(), bytes);
    JsonValue::parse(text).unwrap()
}

fn at_nodes(job: &serde_json::Value, nodes: usize) -> JsonValue {
    let mut value = populated(job, 1);
    value["padding"] = json!([]);
    let base = JsonValue::parse(value.to_string()).unwrap();
    let count = nodes - backend::node_count(base.view());
    value["padding"] = json!(vec![0; count]);
    let value = JsonValue::parse(value.to_string()).unwrap();
    assert_eq!(backend::node_count(value.view()), nodes);
    value
}

#[test]
fn product_real_corpus_values() {
    let Ok(path) = std::env::var("OB_PRODUCT_CORPUS") else {
        return;
    };
    let path = std::path::Path::new(&path);
    let jobs: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let base = path.parent().unwrap().parent().unwrap();
    let mut rows = Vec::new();
    let mut lost = Vec::new();
    for job in jobs.as_array().unwrap() {
        let id = job["id"].as_str().unwrap();
        if !(id.contains("from-template/post/responses/201")
            || id.contains("templates~1{id}/get/responses/200")
            || id.ends_with("/schemas/Page")
            || id.ends_with("/schemas/PageLayout")
            || id == "obi-source-puller#openbindings.source-puller.pullSource/output")
        {
            continue;
        }
        let mut resources = ExternalSchemas::default();
        if let Some(file) = job["resource_file"].as_str() {
            resources = resources
                .with(
                    job["resource_uri"].as_str().unwrap(),
                    JsonValue::parse(std::fs::read_to_string(base.join(file)).unwrap()).unwrap(),
                )
                .unwrap();
        }
        let proof = Document::parse(job["document"].as_str().unwrap())
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        let operation = proof
            .resolve_operation(job["operation"].as_str().unwrap())
            .unwrap();
        let schema = if job["slot"] == "input" {
            operation.input()
        } else {
            operation.output()
        }
        .unwrap();
        compiled::entries::take_applications();
        let selected = schema
            .prepare(
                &DefaultEvaluator::new(),
                &resources,
                &PreparationLimits::default(),
            )
            .unwrap()
            .into_validator();
        let coefficient = compiled::entries::take_applications().unwrap();
        let baseline = DefaultValidator {
            plan: selected.plan.clone(),
            compiled: selected.compiled.without_application_bound(),
        };
        let original = schema
            .prepare(
                &Original(Limits::default()),
                &resources,
                &PreparationLimits::default(),
            )
            .unwrap()
            .into_validator();
        // Retain identical probes on both sides of the initial candidate ceiling.
        let boundary = 10_000_000 / coefficient;
        let mut cases = Vec::new();
        for bytes in [100_000, 1_000_000, 10_000_000] {
            for dense in [false, true] {
                cases.push((
                    format!("{bytes}-{}", if dense { "populated" } else { "text" }),
                    sized(job, bytes, dense),
                ));
            }
        }
        for nodes in [boundary, boundary + 1, 1_000_000] {
            if nodes <= 1_000_000 {
                cases.push((format!("nodes-{nodes}"), at_nodes(job, nodes)));
            }
        }
        for (case, value) in cases {
            let before = measure(&baseline, &value);
            let after = measure(&selected, &value);
            let independent = measure(&original, &value);
            let row = json!({"id":id,"case":case,"bytes":value.bytes().len(),"nodes":backend::node_count(value.view()),
                "coefficient":coefficient,"ceiling":compiled::MAXIMUM_EVALUATION_APPLICATIONS,
                "baseline":before,"selected":after,"independent_original":independent});
            eprintln!("{row}");
            // Save the complete matrix before reporting any lost verdict.
            // Original itself may already decline some legitimate large values.
            assert_eq!(before["outcome"]["status"], "satisfies", "baseline: {row}");
            assert_eq!(before["route"], "compiled", "baseline: {row}");
            assert!(
                independent["outcome"]["status"] == "satisfies"
                    || (independent["outcome"]["status"] == "no-verdict"
                        && independent["outcome"]["detail"]["reason"] == "limit-exceeded"),
                "Original: {row}"
            );
            if after["outcome"]["status"] != "satisfies" {
                lost.push(row.clone());
            }
            rows.push(row);
            write("product-real-values.json", &json!(rows));
        }
    }
    assert_eq!(rows.len(), 45);
    assert!(
        lost.is_empty(),
        "legitimate values lost verdicts: {}",
        json!(lost)
    );
}
