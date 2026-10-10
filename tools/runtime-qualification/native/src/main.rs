use openbindings::{
    Conformance, ContractPreparation, JsonValue, ParsedDocument, ResourceSet, Side, ValueOutcome,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};
use std::{collections::BTreeMap, hint::black_box, path::Path, sync::Arc, time::Instant};
fn measured<T>(enabled: bool, f: impl FnOnce() -> T) -> (T, f64) {
    let start = enabled.then(Instant::now);
    let result = black_box(f());
    (
        result,
        start.map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0),
    )
}
fn once(
    document: &str,
    valid: &str,
    invalid: &str,
    limited: bool,
    timed: bool,
) -> (BTreeMap<&'static str, f64>, Value) {
    let mut row = BTreeMap::new();
    let (document, ms) = measured(timed, || ParsedDocument::parse(document).unwrap());
    row.insert("parseDocument", ms);
    let (assessment, ms) = measured(timed, || document.assess().unwrap());
    row.insert("assess", ms);
    assert_eq!(assessment.report().conclusion, Conformance::Conformant);
    let ((context, contract), ms) = measured(timed, || {
        let context = document
            .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
            .unwrap();
        let ContractPreparation::Ready(contract) = context.prepare("check", Side::Input) else {
            panic!("must prepare")
        };
        (context, contract)
    });
    row.insert("contextAndPrepare", ms);
    let (value, ms) = measured(timed, || JsonValue::parse(valid).unwrap());
    row.insert("parseValue", ms);
    let (first, ms) = measured(timed, || contract.validate(&value));
    row.insert("firstValidation", ms);
    if limited {
        assert!(
            matches!(&first, ValueOutcome::NoVerdict { detail } if detail.code == "evaluation-work-limit")
        );
    } else {
        assert!(matches!(first, ValueOutcome::Satisfies));
    }
    let (hot, ms) = measured(timed, || contract.validate(&value));
    row.insert("hot", ms);
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&hot).unwrap()
    );
    let (invalid, ms) = measured(timed, || JsonValue::parse(invalid).unwrap());
    row.insert("parseInvalid", ms);
    let (failed, ms) = measured(timed, || contract.validate(&invalid));
    row.insert("invalid", ms);
    assert!(matches!(failed, ValueOutcome::Fails { .. }));
    let (wire, ms) = measured(timed, || serde_json::to_string(&failed).unwrap());
    row.insert("serialize", ms);
    let observation = json!({"valid":first,"invalid":serde_json::from_str::<Value>(&wire).unwrap(),"invalidWireBytes":wire.len()});
    let (_, ms) = measured(timed, || {
        drop((
            failed, invalid, hot, first, value, contract, context, assessment, document,
        ))
    });
    row.insert("cleanup", ms);
    row.insert(
        "complete",
        row["parseDocument"]
            + row["assess"]
            + row["contextAndPrepare"]
            + row["parseValue"]
            + row["firstValidation"],
    );
    row.insert("invalidAndSerialize", row["invalid"] + row["serialize"]);
    (row, observation)
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let root = Path::new(&args[1]);
    let timed = args.get(2).is_some_and(|mode| mode == "measure");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    if args.get(2).is_some_and(|mode| mode == "cold") {
        let source = |kind: &str| {
            std::fs::read_to_string(
                root.join(
                    manifest["tiers"]["small"]["files"][kind]["name"]
                        .as_str()
                        .unwrap(),
                ),
            )
            .unwrap()
        };
        let (stages, observation) = once(
            &source("document"),
            &source("valid"),
            &source("invalid"),
            false,
            true,
        );
        println!("{}", json!({"stagesMs":stages,"observation":observation}));
        return;
    }
    let mut rows = BTreeMap::new();
    for (tier, info) in manifest["tiers"].as_object().unwrap() {
        let read = |kind: &str| {
            std::fs::read_to_string(root.join(info["files"][kind]["name"].as_str().unwrap()))
                .unwrap()
        };
        let document = read("document");
        let valid = read("valid");
        let invalid = read("invalid");
        let once = || once(&document, &valid, &invalid, tier == "near", timed);
        once();
        once();
        let reps = if timed {
            info["repetitions"].as_u64().unwrap()
        } else {
            1
        };
        let mut samples: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        let mut observation = Value::Null;
        for _ in 0..if timed { 7 } else { 1 } {
            let mut totals = BTreeMap::new();
            for _ in 0..reps {
                let (row, observed) = once();
                observation = observed;
                for (stage, ms) in row {
                    *totals.entry(stage).or_insert(0.0) += ms;
                }
            }
            for (stage, sum) in totals {
                samples.entry(stage).or_default().push(sum / reps as f64);
            }
        }
        // Avoid retaining every problem in the receipt; preserve exact first coordinate/code.
        let invalid = &observation["invalid"];
        rows.insert(tier.clone(), json!({"samplesMs":if timed {Some(samples)} else {None},"repetitions":reps,"observation":{
            "valid":observation["valid"],"outcome":invalid["outcome"],"problems":invalid["problems"].as_array().unwrap().len(),
            "complete":invalid["problems_complete"],"firstProblem":invalid["problems"][0],"wireBytes":observation["invalidWireBytes"]
        },"concurrency":1,"throughputMeaning":if tier == "near" {"resource refusals"} else {"successful retained validation"}}));
    }
    let source = |kind: &str| {
        std::fs::read_to_string(root.join(manifest["adversarial"][kind]["name"].as_str().unwrap()))
            .unwrap()
    };
    let document = ParsedDocument::parse(&source("document")).unwrap();
    let context = document
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let ContractPreparation::Ready(contract) = context.prepare("check", Side::Input) else {
        panic!("witness prepares")
    };
    let value = JsonValue::parse(&source("invalid")).unwrap();
    let mut samples = vec![];
    let mut observation = Value::Null;
    for n in 0..if timed { 9 } else { 1 } {
        let (result, invalid_ms) = measured(timed, || contract.validate(&value));
        let (wire, serialize_ms) = measured(timed, || serde_json::to_string(&result).unwrap());
        let ValueOutcome::Fails {
            problems,
            problems_complete,
        } = result
        else {
            panic!("witness fails")
        };
        assert!(!problems_complete);
        observation = json!({"outcome":"fails","problems":problems.len(),"complete":problems_complete,"wireBytes":wire.len(),"instancePointerBytes":problems.iter().map(|p|p.instance_pointer.len()).sum::<usize>(),"schemaPointerBytes":problems.iter().map(|p|p.schema_location.as_ref().map_or(0,|s|s.pointer.len())).sum::<usize>()});
        if n >= 2 {
            samples.push(invalid_ms + serialize_ms);
        }
    }
    println!(
        "{}",
        json!({"kind":"native","timed":timed,"tiers":rows,"amplification":{"samplesMs":samples,"observation":observation},"ownership":"public native API exposes no live arena counter; correctness/Drop controls separate; RSS measured by campaign wrapper"})
    );
}
