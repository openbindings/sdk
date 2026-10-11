//! Fixed, local workload evidence, not an end-to-end service benchmark.
use interface_synthesis_consumer::{Catalog, KIND, derive};
use openbindings::{JsonValue, ParsedDocument, Source, WorkControl};
use openbindings_interface_synthesis::*;
use std::{
    future::{Ready, ready},
    hint::black_box,
    sync::{Mutex, atomic::Ordering},
    time::Instant,
};
struct Once(Mutex<Option<SynthesisCandidate>>);
impl InterfaceSynthesizer for Once {
    type Synthesis<'a> = Ready<Result<SynthesisCandidate, SynthesisError>>;
    fn supports_kind(&self, k: &str) -> bool {
        k == KIND
    }
    fn synthesize<'a>(
        &'a self,
        _: &'a SynthesisRequest<'a>,
        _: WorkControl,
    ) -> Self::Synthesis<'a> {
        ready(Ok(self.0.lock().unwrap().take().unwrap()))
    }
}
const REPS: usize = 7;
fn report(units: usize, phase: &str, mut values: Vec<u128>, input: usize, output: usize) {
    values.sort_unstable();
    println!(
        "{units},{phase},{REPS},{},{},{},{input},{output}",
        values[0],
        values[REPS / 2],
        values[REPS - 1]
    );
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("units,phase,repetitions,min_ns,median_ns,max_ns,input_bytes,document_bytes");
    for size in [0, 100, 1000, 10000] {
        let text = format!(
            r#"{{"units":[{}]}}"#,
            (0..size)
                .map(|i| format!(r#"{{"id":"unit{i}"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let source = Source {
            kind: KIND.into(),
            content: Some(JsonValue::parse(&text).unwrap()),
            ..Default::default()
        };
        let inputs = [SynthesisSource {
            name: Some("catalog"),
            source: &source,
        }];
        let request = SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        };
        let control = WorkControl::new();
        let seed = derive(&request, &control, false).unwrap();
        let bytes = seed.document.original_bytes().len();
        let mut derive_times = Vec::new();
        let mut checked = Vec::new();
        let mut warm_validation = Vec::new();
        let mut cold_assessment = Vec::new();
        let mut warm_assessment = Vec::new();
        let mut encoding = Vec::new();
        let provider = Catalog::default();
        let facade = Synthesizer::new(&provider);
        for _ in 0..REPS {
            let t = Instant::now();
            let candidate = derive(&request, &control, false).unwrap();
            derive_times.push(t.elapsed().as_nanos());
            black_box(candidate);
            let t = Instant::now();
            let result = facade.synthesize_with_coverage(&request).await.unwrap();
            checked.push(t.elapsed().as_nanos());
            assert_eq!(result.coverage().entries().len(), size);
            let fresh = ParsedDocument::parse(seed.document.original_bytes()).unwrap();
            let t = Instant::now();
            black_box(fresh.assess().unwrap().validated().unwrap());
            cold_assessment.push(t.elapsed().as_nanos());
            let t = Instant::now();
            black_box(fresh.assess().unwrap());
            warm_assessment.push(t.elapsed().as_nanos());
            let mut candidate = seed.clone();
            candidate.document = fresh;
            let once = Synthesizer::new(Once(Mutex::new(Some(candidate))));
            let t = Instant::now();
            let warm = once.synthesize_with_coverage(&request).await.unwrap();
            warm_validation.push(t.elapsed().as_nanos());
            black_box(warm);
            let t = Instant::now();
            let encoded = result.to_json().unwrap();
            encoding.push(t.elapsed().as_nanos());
            black_box(encoded);
        }
        assert_eq!(provider.calls.load(Ordering::Relaxed), REPS);
        for (phase, times) in [
            ("provider_derivation", derive_times),
            ("complete_checked", checked),
            ("coverage_with_cached_assessment", warm_validation),
            ("assessment_cold", cold_assessment),
            ("assessment_cached", warm_assessment),
            ("envelope_encoding", encoding),
        ] {
            report(size, phase, times, text.len(), bytes)
        }
    }
}
