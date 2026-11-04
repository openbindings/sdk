//! The only module that knows the SDK API. Keep this interface stable across API changes.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const ROOT: &str = "https://benchmark.invalid/root";
#[derive(Clone, Default)]
pub struct Limits(openbindings_json_schema_evaluator::Limits);
// The frozen baseline protocol includes evaluation_depth. It has no current
// SDK counterpart: the candidate replays that historical input under the fixed
// effect profile. Its changed result is adjudicated separately from equivalent
// policy timing comparisons; this does not restore a public compatibility knob.
impl<'de> Deserialize<'de> for Limits {
    fn deserialize<D: serde::Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut request = serde_json::Map::<String, Value>::deserialize(input)?;
        let _historical_depth = match request.remove("evaluation_depth") {
            Some(depth) => Some(
                depth
                    .as_u64()
                    .and_then(|depth| usize::try_from(depth).ok())
                    .ok_or_else(|| D::Error::custom("evaluation_depth is a count"))?,
            ),
            None => None,
        };
        // Only the frozen private request carrier retains these old names.
        // Reject new-only and mixed input rather than enlarging its protocol.
        for (carrier, current) in [
            ("evaluation_steps", "evaluation_sdk_work_units"),
            ("program_bytes", "evaluation_sdk_reservation_bytes"),
        ] {
            if request.contains_key(current) {
                return Err(D::Error::custom(format!(
                    "{current} is not a frozen benchmark carrier key"
                )));
            }
            if let Some(value) = request.remove(carrier) {
                request.insert(current.into(), value);
            }
        }
        let limits: openbindings_json_schema_evaluator::Limits =
            serde_json::from_value(Value::Object(request)).map_err(D::Error::custom)?;
        Ok(Limits(limits))
    }
}

#[cfg(test)]
mod limits_tests {
    use super::*;

    #[test]
    fn frozen_carrier_moves_values_without_accepting_current_sdk_keys() {
        for work in [Value::Null, json!(0), json!(17)] {
            for bytes in [0, 41] {
                let parsed: Limits = serde_json::from_value(json!({
                    "evaluation_steps": work,
                    "program_bytes": bytes,
                }))
                .unwrap();
                let encoded = serde_json::to_value(parsed.0).unwrap();
                assert_eq!(encoded["evaluation_sdk_work_units"], work);
                assert_eq!(encoded["evaluation_sdk_reservation_bytes"], bytes);
            }
        }
        let omitted: Limits = serde_json::from_value(json!({})).unwrap();
        assert_eq!(omitted.0.evaluation_sdk_work_units, None);
        assert_eq!(omitted.0.evaluation_sdk_reservation_bytes, 48 << 20);
        for current in [
            "evaluation_sdk_work_units",
            "evaluation_sdk_reservation_bytes",
        ] {
            for value in [Value::Null, json!(0), json!(17)] {
                let mut keys = json!({});
                keys[current] = value;
                assert!(serde_json::from_value::<Limits>(keys.clone()).is_err());
                keys["evaluation_steps"] = Value::Null;
                keys["program_bytes"] = json!(0);
                assert!(serde_json::from_value::<Limits>(keys).is_err());
            }
        }
        for (key, invalid) in [
            ("evaluation_steps", json!(-1)),
            ("evaluation_steps", json!(1.5)),
            ("evaluation_steps", json!("17")),
            ("program_bytes", Value::Null),
            ("program_bytes", json!(-1)),
            ("program_bytes", json!(1.5)),
        ] {
            let mut keys = json!({});
            keys[key] = invalid;
            assert!(serde_json::from_value::<Limits>(keys).is_err());
        }
    }
}
// Reusable preparation inputs: the proof, one resource set and one evaluator.
pub struct Context {
    document: ConformantDocument,
    resources: ExternalSchemas,
    evaluator: Evaluator,
}
// This harness fixes the requested detail before entering a timed region.
struct Evaluator {
    inner: DefaultEvaluator,
    detail: Detail,
}
struct Validator {
    inner: DefaultValidator,
    detail: Detail,
}
impl SchemaEvaluator for Evaluator {
    type Validator = Validator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Validator, PreparationFailure> {
        self.inner
            .prepare(request, limits, control)
            .map(|inner| Validator {
                inner,
                detail: self.detail,
            })
    }
}
impl SchemaValidator for Validator {
    fn evaluate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        self.inner.evaluate(
            value,
            if self.detail == Detail::Explain {
                Detail::Explain
            } else {
                detail
            },
            control,
        )
    }
}
#[repr(transparent)]
pub struct Prepared(Result<PreparedSchema<Validator>, PreparationFailure>);
#[repr(transparent)]
pub struct ParsedValue(JsonValue);
#[repr(transparent)]
pub struct Outcome(Result<ValueOutcome, PreparationFailure>);

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
        .map(|(uri, text)| ExternalSchema {
            uri: uri.clone(),
            document: JsonValue::parse(text).unwrap(),
        })
        .collect();
    resources.push(ExternalSchema {
        uri: ROOT.into(),
        document: JsonValue::parse(schema).unwrap(),
    });
    let evaluator = DefaultEvaluator::with_limits(limits.0.clone());
    Context {
        document: validation.conformant().unwrap(),
        resources: ExternalSchemas::new(resources).unwrap(),
        evaluator: Evaluator {
            inner: evaluator,
            detail: if upstream {
                Detail::Explain
            } else {
                Detail::Outcome
            },
        },
    }
}
impl Context {
    // Selects the operation by name on every preparation, as the prior context did.
    #[inline(always)]
    pub fn prepare_input(&self) -> Prepared {
        let operation = self.document.resolve_operation("op").unwrap();
        Prepared(operation.input().unwrap().prepare(
            &self.evaluator,
            &self.resources,
            &openbindings::PreparationLimits::default(),
        ))
    }
}
impl Prepared {
    // Timed preparation must produce a usable contract. Refusals are observed
    // separately and must never be counted as successful preparation.
    #[inline(always)]
    pub fn is_ready(&self) -> bool {
        self.0.is_ok()
    }
    // This SDK chooses detail per call.
    #[inline(always)]
    pub fn explain(&self, value: &ParsedValue, cancelled: bool) -> Outcome {
        match &self.0 {
            Ok(contract) => Outcome(Ok(contract
                .explain_with_control(&value.0, &std::sync::atomic::AtomicBool::new(cancelled)))),
            Err(detail) => refused(detail),
        }
    }
    #[inline(always)]
    pub fn validate(&self, value: &ParsedValue, cancelled: bool) -> Outcome {
        match &self.0 {
            Ok(contract) => Outcome(Ok(contract.evaluate_with_control(
                &value.0,
                &std::sync::atomic::AtomicBool::new(cancelled),
            ))),
            Err(detail) => refused(detail),
        }
    }
}
// Observations keep preparation failure distinct from every instance outcome.
#[inline(never)]
fn refused(detail: &PreparationFailure) -> Outcome {
    Outcome(Err(detail.clone()))
}
impl Outcome {
    #[inline(always)]
    pub fn status(&self) -> &'static str {
        let Ok(outcome) = &self.0 else {
            return "preparation-refused";
        };
        match outcome {
            ValueOutcome::Satisfies => "satisfies",
            ValueOutcome::Fails { .. } => "fails",
            ValueOutcome::Undefined { .. } => "undefined",
            ValueOutcome::NoVerdict { .. } => "no-verdict",
        }
    }
    pub fn observation(&self) -> Value {
        let mut value = json!({"status": self.status()});
        let Ok(outcome) = &self.0 else {
            value["failure"] = serde_json::to_value(self.0.as_ref().unwrap_err()).unwrap();
            return value;
        };
        match outcome {
            ValueOutcome::NoVerdict { detail } => {
                value["code"] = json!(detail.code);
                value["detail"] = serde_json::to_value(detail).unwrap();
                // The instrument's location protocol is fixed across SDK revisions.
                if let Some(location) = value["detail"]["location"].as_object_mut()
                    && let Some(uri) = location.remove("source_uri")
                {
                    location.insert("resource".into(), uri);
                }
            }
            ValueOutcome::Undefined { detail } => {
                value["code"] = json!(detail.code);
                value["detail"] = serde_json::to_value(detail).unwrap();
                // The instrument's location protocol is fixed across SDK revisions.
                if let Some(location) = value["detail"]["location"].as_object_mut()
                    && let Some(uri) = location.remove("source_uri")
                {
                    location.insert("resource".into(), uri);
                }
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
                            "instance_pointer": problem.instance.to_string(),
                            "schema_location": {
                                "resource": problem.schema.source_uri,
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
    proof: Option<ConformantDocument>,
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
