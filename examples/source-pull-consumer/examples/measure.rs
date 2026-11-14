//! Current source-pull scale observations, not an old synthesis timing oracle.
use openbindings::Document;
use openbindings_source_pull::{
    CancellationToken, PullLimits, PullRequest, SourcePuller, SourcePullerExt,
};
use source_pull_consumer::Embedded;
use std::{hint::black_box, time::Instant};

const REPS: usize = 7;

fn row(items: usize, phase: &str, mut times: Vec<u128>, input: usize, output: usize) {
    times.sort_unstable();
    println!(
        "{}",
        serde_json::json!({"items":items,"phase":phase,"repetitions":REPS,
            "min_ns":times[0],"median_ns":times[REPS/2],"max_ns":times[REPS-1],
            "request_document_bytes":input,"prospective_document_bytes":output})
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    for size in [0, 100, 1_000, 10_000] {
        let items = (0..size)
            .map(|i| format!(r#"{{"key":"unit{i}","operation":{{"input":true,"output":true}},"data":{{"item":"unit{i}"}}}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!(
            r#"{{"openbindings":"0.2.0","operations":{{}},"sources":{{"catalog":{{"kind":"example.embedded@1","data":{{"items":[{items}]}}}}}}}}"#
        );
        let original = Document::parse(&text)
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        let request = PullRequest::new(original.clone(), "catalog");
        let provider = Embedded;
        let control = CancellationToken::new();
        let seed = provider
            .handle_pull_source(request.clone(), control.clone())
            .await
            .unwrap();
        assert_eq!(seed.report().added().len(), size);
        assert!(seed.report().is_complete());
        let limits = PullLimits::default();
        let adopted = seed.validate_for(&request, &limits, &control).unwrap_or_else(|failure| {
            // A resource refusal is a failed scale lane, never a faster success.
            eprintln!("{}", serde_json::json!({"items":size,"status":"refused","failure":format!("{failure:?}")}));
            panic!("default policy did not complete the requested scale workload");
        });
        let bytes = adopted.as_bytes().len();
        assert_eq!(adopted.operations().len(), size);
        assert_eq!(adopted.value().get("bindings").unwrap().len(), Some(size));
        assert!(original.value().get("bindings").is_none());
        let client = &provider;
        let mut projection = Vec::new();
        let mut checked = Vec::new();
        let mut validation = Vec::new();
        let mut cold = Vec::new();
        let mut cached = Vec::new();
        let mut encoding = Vec::new();
        for _ in 0..REPS {
            let start = Instant::now();
            let result = provider
                .handle_pull_source(request.clone(), control.clone())
                .await
                .unwrap();
            projection.push(start.elapsed().as_nanos());
            black_box(result);
            let start = Instant::now();
            let (result, _) = client
                .pull_source(request.clone(), control.clone())
                .await
                .unwrap();
            checked.push(start.elapsed().as_nanos());
            assert_eq!(result.report().added().len(), size);
            let start = Instant::now();
            black_box(seed.validate_for(&request, &limits, &control).unwrap());
            validation.push(start.elapsed().as_nanos());
            let fresh = Document::parse(adopted.as_bytes()).unwrap();
            let start = Instant::now();
            black_box(fresh.validate().unwrap().conformant().unwrap());
            cold.push(start.elapsed().as_nanos());
            let start = Instant::now();
            let _ = black_box(fresh.validate().unwrap());
            cached.push(start.elapsed().as_nanos());
            let start = Instant::now();
            black_box(serde_json::to_string(result.value().value()).unwrap());
            encoding.push(start.elapsed().as_nanos());
        }
        for (phase, times) in [
            ("provider_projection", projection),
            ("complete_checked_pull", checked),
            ("relationships_and_prospective_conformance", validation),
            ("conformance_cold", cold),
            ("conformance_cached", cached),
            ("report_encoding", encoding),
        ] {
            row(size, phase, times, text.len(), bytes);
        }
    }
}
