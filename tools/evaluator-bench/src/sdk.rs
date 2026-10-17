//! The only module that knows the SDK API. Keep this interface stable across API changes.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

pub const ROOT: &str = "https://benchmark.invalid/root";
#[derive(Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct Limits(openbindings_json_schema_evaluator::Limits);
// Reusable preparation inputs: the proof, one resource set and one evaluator.
pub struct Context {
    document: Conformant<Document>,
    resources: ResourceSet,
    evaluator: Evaluator,
}
// A request for upstream diagnostics asks every check of the contract to
// explain: its evaluator answers each validation as an explanation.
enum Evaluator {
    Default(DefaultEvaluator),
    Explaining(Explaining),
}
impl Evaluator {
    #[inline(always)]
    fn get(&self) -> &dyn SchemaEvaluator {
        match self {
            Self::Default(evaluator) => evaluator,
            Self::Explaining(evaluator) => evaluator,
        }
    }
}
struct Explaining(DefaultEvaluator);
struct Explained(Arc<dyn PreparedSchema>);
impl SchemaEvaluator for Explaining {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(Explained(self.0.prepare(request, control)?)))
    }
}
impl PreparedSchema for Explained {
    fn validate(&self, value: &JsonValue, _: Detail, control: &dyn WorkControl) -> ValueOutcome {
        self.0.validate(value, Detail::Explain, control)
    }
}
#[repr(transparent)]
pub struct Prepared(Result<PreparedContract, NoVerdict>);
#[repr(transparent)]
pub struct ParsedValue(JsonValue);
#[repr(transparent)]
pub struct Outcome(ValueOutcome);

#[inline(always)]
pub fn parse_value(text: &str) -> ParsedValue {
    ParsedValue(JsonValue::parse(text).unwrap())
}
pub fn context(
    schema: &str,
    supplied: &BTreeMap<String, String>,
    limits: &Limits,
    upstream: bool,
) -> Context {
    let document = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$ref":"{ROOT}"}}}}}}}}"#
    ))
    .unwrap();
    let validation = document.validate().unwrap();
    let mut resources: Vec<_> = supplied
        .iter()
        .map(|(uri, text)| SchemaResource {
            uri: uri.clone(),
            document: JsonValue::parse(text).unwrap(),
        })
        .collect();
    resources.push(SchemaResource {
        uri: ROOT.into(),
        document: JsonValue::parse(schema).unwrap(),
    });
    let evaluator = DefaultEvaluator::with_limits(limits.0.clone());
    Context {
        document: validation.conformant().unwrap(),
        resources: ResourceSet::new(resources).unwrap(),
        evaluator: if upstream {
            Evaluator::Explaining(Explaining(evaluator))
        } else {
            Evaluator::Default(evaluator)
        },
    }
}
impl Context {
    // Selects the operation by name on every preparation, as the prior context did.
    #[inline(always)]
    pub fn prepare_input(&self) -> Prepared {
        let operation = self.document.resolve_operation("op").unwrap();
        Prepared(
            operation
                .input()
                .unwrap()
                .prepare_with(self.evaluator.get(), &self.resources),
        )
    }
}
impl Prepared {
    #[inline(always)]
    pub fn is_ready(&self) -> bool {
        self.0.is_ok()
    }
    // This SDK chooses detail per call.
    #[inline(always)]
    pub fn explain(&self, value: &ParsedValue, cancelled: bool) -> Outcome {
        match &self.0 {
            Ok(contract) => Outcome(
                contract
                    .explain_with_control(&value.0, &std::sync::atomic::AtomicBool::new(cancelled)),
            ),
            Err(detail) => panic!("fixture did not prepare: {detail:?}"),
        }
    }
    #[inline(always)]
    pub fn validate(&self, value: &ParsedValue, cancelled: bool) -> Outcome {
        match &self.0 {
            Ok(contract) => {
                Outcome(contract.validate_with_control(
                    &value.0,
                    &std::sync::atomic::AtomicBool::new(cancelled),
                ))
            }
            Err(detail) => panic!("fixture did not prepare: {detail:?}"),
        }
    }
}
impl Outcome {
    #[inline(always)]
    pub fn status(&self) -> &'static str {
        match &self.0 {
            ValueOutcome::Satisfies => "satisfies",
            ValueOutcome::Fails { .. } => "fails",
            ValueOutcome::Undefined { .. } => "undefined",
            ValueOutcome::NoVerdict { .. } => "no-verdict",
        }
    }
    pub fn observation(&self) -> Value {
        let mut value = json!({"status": self.status()});
        match &self.0 {
            ValueOutcome::NoVerdict { detail } => {
                value["code"] = json!(detail.code);
                value["detail"] = serde_json::to_value(detail).unwrap();
            }
            ValueOutcome::Undefined { detail } => {
                value["code"] = json!(detail.code);
                value["detail"] = serde_json::to_value(detail).unwrap();
            }
            ValueOutcome::Fails {
                problems,
                problems_complete,
                ..
            } => {
                // Observations keep one shape across SDK revisions: each
                // problem's instance pointer and schema location.
                let problems: Vec<Value> = problems
                    .iter()
                    .map(|problem| {
                        let mut observed = json!({
                            "instance_pointer": problem
                                .instance
                                .pointer
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_default(),
                            "schema_location": {
                                "resource": problem.schema.resource,
                                "pointer": problem.schema.pointer.to_string(),
                            },
                            "code": problem.code,
                            "message": problem.message,
                        });
                        if let Some(details) = &problem.details {
                            observed["details"] = serde_json::to_value(details).unwrap();
                        }
                        observed
                    })
                    .collect();
                value["problems"] = json!(problems);
                value["problems_complete"] = json!(problems_complete);
            }
            _ => {}
        }
        value
    }
}

// Both owners are retained: document timing includes acquiring the proof, and
// read timing starts from that proof without repeating validation.
pub struct CheckedDocument {
    validation: DocumentValidation,
    proof: Option<Conformant<Document>>,
}
pub fn validate_document_bytes(bytes: &[u8]) -> CheckedDocument {
    let validation = validate_document(bytes).unwrap();
    let proof = validation.conformant();
    CheckedDocument { validation, proof }
}
#[derive(Default, serde::Serialize)]
pub struct Reads {
    operations: usize,
    aliases: usize,
    descriptions: usize,
    tags: usize,
    inputs: usize,
    outputs: usize,
    text_bytes: usize,
}
impl CheckedDocument {
    #[inline(always)]
    pub fn status(&self) -> &'static str {
        if self.proof.is_some() {
            "conformant"
        } else {
            "non-conformant"
        }
    }
    pub fn observation(&self) -> Value {
        let report = self.validation.report();
        json!({"status": self.status(), "finding_count": report.findings.len(), "report": report})
    }
    pub fn read(&self) -> Reads {
        use std::hint::black_box;
        let mut reads = Reads::default();
        for operation in self
            .proof
            .as_ref()
            .expect("read requires conformance")
            .operations()
        {
            reads.operations += 1;
            reads.text_bytes += black_box(operation.key()).len();
            if let Some(aliases) = operation.aliases() {
                for alias in aliases {
                    reads.aliases += 1;
                    reads.text_bytes += black_box(alias).len();
                }
            }
            if let Some(description) = operation.description() {
                reads.descriptions += 1;
                reads.text_bytes += black_box(description).len();
            }
            if let Some(tags) = operation.tags() {
                for tag in tags {
                    reads.tags += 1;
                    reads.text_bytes += black_box(tag).len();
                }
            }
            reads.inputs += usize::from(black_box(operation.input()).is_some());
            reads.outputs += usize::from(black_box(operation.output()).is_some());
        }
        reads
    }
}
