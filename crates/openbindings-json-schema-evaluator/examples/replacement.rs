//! Application-owned replacement; run with --example replacement.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{error::Error, fmt, sync::mpsc};

const DOCUMENT: &str = r#"{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"],"input":{"$ref":"https://example.invalid/input"},"output":true}}}"#;

#[derive(Debug)]
enum LoadFailure {
    Parse(InputError),
    Version(VersionRefusal),
    Conformance(ValidationReport),
    MissingOperation,
    NoInputContract,
    Preparation(NoVerdict),
    MissingResources(NoVerdict),
    UnresolvedReferences(Box<ReferenceReport>),
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
            Self::MissingOperation => f.write_str("operation find is missing"),
            Self::NoInputContract => f.write_str("operation find declares no input contract"),
            Self::Preparation(detail) => write!(f, "input contract preparation: {detail:?}"),
            Self::MissingResources(detail) => write!(f, "input contract resources: {detail:?}"),
            Self::UnresolvedReferences(report) => {
                write!(f, "unresolved reference inventory: {report:?}")
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

// This application requires conformance and a fully located reference inventory.
// Inspection is an application policy, not a proof that every value is decidable.
fn candidate(
    text: &str,
    resources: ResourceSet,
    control: &dyn WorkControl,
) -> Result<PreparedContract, LoadFailure> {
    let document = Document::parse(text).map_err(LoadFailure::Parse)?;
    let validation = document.validate().map_err(LoadFailure::Version)?;
    let Some(proof) = validation.conformant() else {
        return Err(LoadFailure::Conformance(validation.report().clone()));
    };
    let report = proof.references_with_resources(resources.clone(), control);
    if !report.complete
        || report
            .references
            .iter()
            .any(|r| matches!(r.resolution, ReferenceResolution::Unresolved { .. }))
    {
        return Err(LoadFailure::UnresolvedReferences(Box::new(report)));
    }
    // Retain the distinct missing-operation, no-contract and no-verdict states.
    let operation = proof
        .resolve_operation("find")
        .ok_or(LoadFailure::MissingOperation)?;
    let contract = operation.input().ok_or(LoadFailure::NoInputContract)?;
    match contract.prepare_with_control(&DefaultEvaluator::new(), &resources, control) {
        // Document, resource and evaluator owners end; the returned owner remains usable.
        Ok(input) => Ok(input),
        Err(detail) if detail.reason == NoVerdictReason::ResourceUnavailable => {
            Err(LoadFailure::MissingResources(detail))
        }
        Err(detail) => Err(LoadFailure::Preparation(detail)),
    }
}

fn satisfies(input: &PreparedContract, value: &JsonValue) {
    assert!(matches!(input.validate(value), ValueOutcome::Satisfies));
}
fn mismatch(input: &PreparedContract, value: &JsonValue) {
    assert!(matches!(input.validate(value), ValueOutcome::Fails { .. }));
}

fn main() -> Result<(), Box<dyn Error>> {
    let healthy = std::sync::atomic::AtomicBool::new(false);
    let old_value = JsonValue::parse("9007199254740993")?;
    let new_value = JsonValue::from_serializable(&9_007_199_254_740_994_u64)?;
    // Fixture-only retention demonstration: reusable candidate loading requires no output.
    {
        let document = Document::parse(DOCUMENT)?;
        let validation = document.validate()?;
        let proof = validation
            .conformant()
            .ok_or("fixture must be conformant")?;
        let evaluator = DefaultEvaluator::new();
        let resources = resources(r#"{"const":9007199254740993}"#)?;
        let find = proof
            .resolve_operation("find")
            .expect("fixture declares find");
        let retained = find
            .input()
            .expect("fixture declares an input")
            .prepare_with(&evaluator, &resources)
            .expect("fixture must have a prepared input");
        let lookup = proof
            .resolve_operation("lookup")
            .expect("fixture declares lookup");
        assert!(
            lookup
                .output()
                .expect("fixture declares an output")
                .prepare_with(&evaluator, &resources)
                .is_ok()
        ); // each direction prepares independently
        // The prepared owner survives the document, resources and evaluator.
        drop((find, lookup, proof, validation, document));
        drop((resources, evaluator));
        satisfies(&retained, &old_value);
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
    old_job.join().expect("old preparation remains usable");

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
        Err(LoadFailure::UnresolvedReferences(report)) => {
            assert!(report.references.iter().any(|r| matches!(
                &r.resolution,
                ReferenceResolution::Unresolved { detail }
                    if detail.reason == ReferenceRefusalReason::ResourceUnavailable
            )))
        }
        other => panic!("expected missing-resource rejection: {other:?}"),
    }
    satisfies(&active, &new_value);
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    // Cancellation is observed at the first cooperative boundary, reference inspection.
    match candidate(DOCUMENT, resources(r#"{"const":7}"#)?, &cancelled) {
        Err(LoadFailure::UnresolvedReferences(report)) => {
            assert!(!report.complete);
            assert!(matches!(
                &report.limitation,
                Some(refusal) if refusal.reason == ReferenceRefusalReason::Cancelled
            ));
        }
        other => panic!("expected cancelled loading: {other:?}"),
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
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    satisfies(&active, &old_value);
    drop(active); // old job and every earlier preparation have already released their owners
    println!("retained work, failed replacement, cancellation and healthy recovery passed");
    Ok(())
}
