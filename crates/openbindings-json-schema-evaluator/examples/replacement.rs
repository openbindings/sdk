//! Application-owned replacement; run with --example replacement.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    error::Error,
    fmt,
    sync::{Arc, mpsc},
};

const DOCUMENT: &str = r#"{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"],"input":{"$ref":"https://example.invalid/input"},"output":true}}}"#;

#[derive(Debug)]
enum LoadFailure {
    Parse(InputError),
    Version(VersionRefusal),
    Conformance(ValidationReport),
    Interpretation(InterpretationError),
    Preparation(ValueContractPreparation),
    MissingResources(NoVerdict),
    UndeclaredResources,
}
impl fmt::Display for LoadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "document syntax: {error}"),
            Self::Version(error) => write!(f, "document version: {error}"),
            Self::Conformance(report) => write!(
                f,
                "document conformance: {:?}; findings: {:?}",
                report.conclusion, report.findings
            ),
            Self::Interpretation(error) => write!(f, "document interpretation: {error}"),
            Self::Preparation(state) => write!(f, "input contract setup: {state:?}"),
            Self::MissingResources(detail) => write!(f, "input contract resources: {detail:?}"),
            Self::UndeclaredResources => {
                f.write_str("evaluator did not declare resource completeness")
            }
        }
    }
}
impl Error for LoadFailure {}

fn resources(schema: &str) -> Result<ResourceSet, Box<dyn Error>> {
    Ok(ResourceSet::new([SchemaResource {
        uri: "https://example.invalid/input".into(),
        document: JsonValue::parse(schema)?,
    }])?)
}

// This application requires conformance and complete resources before replacement.
fn candidate(
    text: &str,
    resources: ResourceSet,
    control: &WorkControl,
) -> Result<ValueContract, LoadFailure> {
    let document = ParsedDocument::parse(text).map_err(LoadFailure::Parse)?;
    let validation = document.validate().map_err(LoadFailure::Version)?;
    let Some(proof) = validation.validated() else {
        return Err(LoadFailure::Conformance(validation.report().clone()));
    };
    let context = proof
        .parsed()
        .value_contracts(Arc::new(DefaultEvaluator::new()), resources)
        .map_err(LoadFailure::Interpretation)?;
    let input = match context.prepare_with_control("find", Direction::Input, control) {
        ValueContractPreparation::Ready(input) => input,
        ValueContractPreparation::NoVerdict { detail }
            if detail.reason == NoVerdictReason::ResourceUnavailable =>
        {
            return Err(LoadFailure::MissingResources(detail));
        }
        // Retain the distinct no-contract, missing, ambiguous or no-verdict state.
        other => return Err(LoadFailure::Preparation(other)),
    };
    match input.resource_completeness() {
        ResourceCompleteness::Complete => Ok(input),
        ResourceCompleteness::Incomplete { evidence, .. } => {
            Err(LoadFailure::MissingResources(evidence.clone()))
        }
        _ => Err(LoadFailure::UndeclaredResources),
    } // document/context owners end; the returned owner remains usable.
}

fn satisfies(input: &ValueContract, value: &JsonValue) {
    assert!(matches!(input.validate(value), ValueOutcome::Satisfies));
}
fn mismatch(input: &ValueContract, value: &JsonValue) {
    assert!(matches!(input.validate(value), ValueOutcome::Fails { .. }));
}

fn main() -> Result<(), Box<dyn Error>> {
    let healthy = WorkControl::new();
    let old_value = JsonValue::parse("9007199254740993")?;
    let new_value = JsonValue::from_serializable(&9_007_199_254_740_994_u64)?;
    // Fixture-only retention demonstration: reusable candidate loading requires no output.
    {
        let document = ParsedDocument::parse(DOCUMENT)?;
        let validation = document.validate()?;
        let proof = validation.validated().ok_or("fixture must be conformant")?;
        let context = proof.parsed().value_contracts(
            Arc::new(DefaultEvaluator::new()),
            resources(r#"{"const":9007199254740993}"#)?,
        )?;
        let ValueContractPreparation::Ready(retained) = context.prepare("find", Direction::Input)
        else {
            panic!("fixture must have a ready input");
        };
        assert!(matches!(
            context.prepare("lookup", Direction::Output),
            ValueContractPreparation::Ready(_)
        )); // each direction prepares independently
        drop(context);
        satisfies(&retained, &old_value); // explicit owner survives context drop
    }
    // A caller can also load an input-only operation through the reusable helper.
    let input_only = candidate(
        &DOCUMENT.replace(",\"output\":true", ""),
        resources(r#"{"const":9007199254740993}"#)?,
        &healthy,
    )?;
    satisfies(&input_only, &old_value);
    drop(input_only);
    let mut active = candidate(
        DOCUMENT,
        resources(r#"{"const":9007199254740993}"#)?,
        &healthy,
    )?;
    for _ in 0..3 {
        satisfies(&active, &old_value);
    }
    // The application controls scheduling. The old job waits until after the swap.
    let old_work = active.clone();
    let (resume, wait) = mpsc::channel();
    let old_job = std::thread::spawn(move || {
        wait.recv().expect("application releases old job");
        satisfies(
            &old_work,
            &JsonValue::parse("9007199254740993").expect("fixture"),
        );
        mismatch(
            &old_work,
            &JsonValue::parse("9007199254740994").expect("fixture"),
        );
    });
    // Same URI, different immutable schema: only newly prepared work sees the change.
    active = candidate(
        DOCUMENT,
        resources(r#"{"const":9007199254740994}"#)?,
        &healthy,
    )?;
    satisfies(&active, &new_value);
    mismatch(&active, &old_value);
    resume.send(())?;
    old_job.join().expect("old context remains usable");

    let malformed = DOCUMENT.replace("[\"find\"]", "[false]");
    match candidate(&malformed, ResourceSet::default(), &healthy) {
        Err(LoadFailure::Conformance(report)) => {
            println!("replacement rejected: {:?}", report.findings)
        }
        other => panic!("expected malformed-field rejection: {other:?}"),
    }
    satisfies(&active, &new_value); // failure never assigns to the active slot
    // Preparation can succeed with partial resources; this application checks
    // declared completeness before accepting the candidate, without a probe value.
    match candidate(DOCUMENT, ResourceSet::default(), &healthy) {
        Err(LoadFailure::MissingResources(detail)) => {
            assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable)
        }
        other => panic!("expected missing-resource rejection: {other:?}"),
    }
    satisfies(&active, &new_value);
    let cancelled = WorkControl::new();
    cancelled.cancel();
    match candidate(DOCUMENT, resources(r#"{"const":7}"#)?, &cancelled) {
        Err(LoadFailure::Preparation(ValueContractPreparation::NoVerdict { detail })) => {
            assert_eq!(detail.reason, NoVerdictReason::Cancelled);
        }
        other => panic!("expected cancelled preparation: {other:?}"),
    }
    assert!(matches!(
        active.validate_with_control(&new_value, &cancelled),
        ValueOutcome::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::Cancelled,
                ..
            }
        }
    ));
    satisfies(&active, &new_value); // cancellation has not poisoned the retained owner
    active = candidate(
        DOCUMENT,
        resources(r#"{"const":9007199254740993}"#)?,
        &WorkControl::new(),
    )?;
    satisfies(&active, &old_value);
    drop(active); // old job and every context have already released their owners
    println!("retained work, failed replacement, cancellation and healthy recovery passed");
    Ok(())
}
