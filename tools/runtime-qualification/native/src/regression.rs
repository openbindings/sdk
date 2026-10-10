use openbindings::{
    Conformance, ContractPreparation, Evidence, JsonValue, ParsedDocument, ResourceSet, Side,
    ValueOutcome,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;
use std::{hint::black_box, sync::Arc, time::Instant};

fn prepare(source: &str) -> openbindings::PreparedContract {
    let document = ParsedDocument::parse(source).unwrap();
    let context = document
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let ContractPreparation::Ready(contract) = context.prepare("run", Side::Input) else {
        panic!("contract must prepare")
    };
    contract
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let (variant, name, lane) = (&args[1], &args[2], &args[3]);
    let source = std::fs::read_to_string(&args[4]).unwrap();
    let contract_job = name == "contract-wide";
    let repetitions = if lane == "retained" || name == "healthy" {
        100
    } else if name.contains("prefix") {
        1
    } else {
        5
    };
    let value = JsonValue::parse(r#"{"n":1e1000}"#).unwrap();
    let retained = contract_job.then(|| prepare(&source));
    let observation = if contract_job {
        let outcome = retained.as_ref().unwrap().validate(&value);
        assert!(matches!(outcome, ValueOutcome::Satisfies));
        json!({"outcome":"satisfies"})
    } else {
        let document = ParsedDocument::parse(&source).unwrap();
        let assessment = document.assess().unwrap();
        let report = assessment.report();
        if name == "healthy" {
            assert_eq!(report.conclusion, Conformance::Conformant);
            assert!(report.findings.is_empty() && !report.findings_truncated);
        } else if name.starts_with("duplicates") {
            let pointer = if name == "duplicates-root" {
                String::new()
            } else if name == "duplicates-prefix" {
                "/x-duplicates".to_owned()
            } else {
                "/x-".to_owned() + &"a".repeat(4096)
            };
            // Both adoption baseline and candidate already contain the document pointer cap.
            let expected_count = if name == "duplicates-wide-key" {
                (8 * 1024 * 1024) / pointer.len()
            } else {
                4096
            };
            assert_eq!(report.findings.len(), expected_count);
            assert_eq!(report.evidence["OBI-01"], Evidence::Violated);
            assert_eq!(report.evidence["OBI-02"], Evidence::NotApplicable);
            assert!(report.findings_truncated);
            let offset = source.find("{\"k\"").unwrap();
            for f in &report.findings {
                let at = f.location.as_ref().unwrap();
                assert_eq!(f.code, "duplicate-member");
                assert_eq!(at.pointer.as_ref(), Some(&pointer));
                assert_eq!(at.byte_offset, offset);
            }
        } else {
            assert_eq!(name, "names-prefix");
            assert_eq!(report.evidence["OBI-05"], Evidence::Violated);
            assert_eq!(report.findings.len(), 4096);
            assert!(report.findings_truncated);
            let mut offset = source.find("\"aliases\":[").unwrap() + "\"aliases\":[".len();
            for (index, f) in report.findings.iter().enumerate() {
                assert_eq!(f.code, "duplicate-operation-name");
                let at = f.location.as_ref().unwrap();
                assert_eq!(
                    at.pointer.as_deref(),
                    Some(format!("/operations/run/aliases/{index}").as_str())
                );
                assert_eq!(at.byte_offset, offset);
                assert_eq!(at.line, index + 2);
                offset += format!("\"op{index}\",\n").len();
            }
        }
        json!({"conclusion":report.conclusion,"evidence":report.evidence,"findings":report.findings.len(),"truncated":report.findings_truncated,"pointer_bytes":report.findings.iter().map(|f|f.location.as_ref().and_then(|at|at.pointer.as_ref()).map_or(0,String::len)).sum::<usize>()})
    };
    if args.get(5).is_some_and(|x| x == "--check") {
        println!(
            "{}",
            json!({"variant":variant,"fixture":name,"observation":observation})
        );
        return;
    }
    let once = || {
        if lane == "retained" {
            let start = Instant::now();
            let outcome = black_box(retained.as_ref().unwrap().validate(black_box(&value)));
            let elapsed = start.elapsed().as_nanos();
            drop(outcome);
            elapsed
        } else if lane == "contract" {
            let start = Instant::now();
            let contract = prepare(black_box(&source));
            let value = JsonValue::parse(r#"{"n":1e1000}"#).unwrap();
            let outcome = black_box(contract.validate(&value));
            let elapsed = start.elapsed().as_nanos();
            drop((outcome, value, contract));
            elapsed
        } else if lane == "assess" {
            let document = ParsedDocument::parse(black_box(&source)).unwrap();
            let start = Instant::now();
            let assessment = black_box(document.assess().unwrap());
            let elapsed = start.elapsed().as_nanos();
            drop((assessment, document));
            elapsed
        } else {
            assert_eq!(lane, "parse-assess");
            let start = Instant::now();
            let document = ParsedDocument::parse(black_box(&source)).unwrap();
            let assessment = black_box(document.assess().unwrap());
            let elapsed = start.elapsed().as_nanos();
            drop((assessment, document));
            elapsed
        }
    };
    for _ in 0..2 {
        black_box(once());
    }
    let samples: Vec<_> = (0..7)
        .map(|_| (0..repetitions).map(|_| once()).sum::<u128>() / repetitions)
        .collect();
    println!(
        "{}",
        json!({"variant":variant,"fixture":name,"lane":lane,"repetitions":repetitions,"samples_ns":samples,"observation":observation})
    );
}
