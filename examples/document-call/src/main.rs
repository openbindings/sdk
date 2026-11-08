use openbindings::{Detail, Document, ExternalSchemas, JsonValue};
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, DefaultOperationInvoker, InvocationError,
    InvocationSupport, OutputEvent, PreflightOutcome, ProviderIo,
    operation::{BindingChoice, OperationPreparationFailure, ValidationEvidence},
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde::Deserialize;
use std::{
    error::Error,
    future::{Future, Ready, ready},
    pin::Pin,
};

const DOCUMENT: &str = r##"{
  "openbindings":"0.2.0",
  "operations":{"echo":{"aliases":["repeat"],
    "input":{"type":"object","properties":{"count":{"type":"integer"}},"required":["count"]},
    "output":{"$ref":"#/operations/echo/input"}}},
  "sources":{"local":{"kind":"example.echo@1"}},
  "bindings":{"echo.local":{"operation":"echo","source":"local"}}
}"##;

// This fictional kind accepts absent source/binding data and echoes every input.
struct Echo;
impl BindingInvoker for Echo {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;

    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            let binding = request
                .interface
                .binding(&request.binding)
                .ok_or_else(|| InvocationError::new("ERR_BINDING_NOT_FOUND", None).unwrap())?;
            let source = binding.source();
            if source.kind() != "example.echo@1"
                || source.data().is_some()
                || binding.data().is_some()
            {
                return Err(InvocationError::refused()); // No operation effect occurred.
            }
            while let Some(value) = io.next().await {
                io.send(value).await?;
            }
            Ok(()) // The SDK emits Complete; outputs are values, not terminal frames.
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        _: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        ready(Ok(PreflightOutcome::NoPrediction)) // No invocation or input consumption.
    }
}
impl InvocationSupport for Echo {
    fn can_invoke(&self, kind: &str) -> bool {
        kind == "example.echo@1"
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct Reply {
    count: u16,
}

async fn call(text: &str, input_text: &str) -> Result<Reply, Box<dyn Error>> {
    let draft = Document::parse(text).map_err(|e| format!("JSON admission: {e}"))?;
    let checked = draft
        .validate()
        .map_err(|e| format!("unsupported version: {e}"))?;
    for finding in &checked.report().findings {
        let pointer = finding.location.as_ref().and_then(|at| at.pointer.as_ref());
        eprintln!(
            "{}/{} at {:?}: {}",
            finding.rule,
            finding.code,
            pointer.map(ToString::to_string),
            finding.message
        );
    }
    if checked.report().findings_truncated {
        eprintln!("Findings were truncated.");
    }
    let interface = checked
        .conformant()
        .ok_or_else(|| format!("document conformance: {:?}", checked.report().conclusion))?;
    let operation = interface
        .resolve_operation("repeat")
        .ok_or("operation is missing")?;
    let adapter = Echo;
    let invoker = DefaultOperationInvoker::new(
        &adapter,
        DefaultEvaluator::new(),
        ExternalSchemas::default(),
    )
    .with_validation_detail(Detail::Explain);
    let control = CancellationToken::new(); // A fresh token for this attempt.
    // Prepares both declared input and output schemas before any provider work.
    let prepared = invoker
        .prepare(interface, operation.key(), &control)
        .map_err(|e| match e {
            OperationPreparationFailure::Admission(cause) => format!("address admission: {cause}"),
            OperationPreparationFailure::Resolution(cause) => {
                format!("operation resolution: {cause}")
            }
            OperationPreparationFailure::Cancelled => "preparation cancelled".to_owned(),
            OperationPreparationFailure::Schema(failure) => match failure.evidence() {
                ValidationEvidence::Preparation(cause) => {
                    format!("{} preparation: {cause}", failure.direction().as_str())
                }
                ValidationEvidence::Evaluation(_) => unreachable!("preparation judges no value"),
            },
        })?;
    let value = JsonValue::parse(input_text).map_err(|e| format!("input JSON admission: {e}"))?;
    let call = prepared
        .invoke(&adapter, BindingChoice::Sole, None, control)
        .map_err(|e| format!("request admission: {e}"))?;
    let receipt = call.control();
    let (mut input, mut output) = call.split();
    // One value fits the default queue. For streams, poll sending and receiving concurrently.
    input
        .send(value)
        .await
        .map_err(|e| format!("input send: {e}"))?;
    input.close();
    let mut values = Vec::new();
    while let Some(event) = output.next().await {
        match event {
            OutputEvent::Output(value) => values.push(value),
            OutputEvent::InputClosed => (),
            OutputEvent::Complete => {
                if receipt.undelivered_inputs() != Some(0) || values.len() != 1 {
                    return Err("echo did not deliver one reply for the accepted input".into());
                }
                // Decode only after success. Schema satisfaction does not prove a u16 fits.
                return serde_json::from_str(values[0].text())
                    .map_err(|e| format!("typed output decoding: {e}").into());
            }
            OutputEvent::Error(error) => {
                if let Some(challenge) = error.context_required() {
                    // Retain for host-controlled acquisition; never retry or disclose automatically.
                    let _context_schema = challenge.schema();
                    eprintln!("Context is required for a new attempt.");
                }
                if let Some(failure) = receipt.validation_failure() {
                    eprintln!("{} validation", failure.direction().as_str());
                    match failure.evidence() {
                        ValidationEvidence::Preparation(cause) => eprintln!("{cause}"),
                        ValidationEvidence::Evaluation(outcome) => eprintln!("{outcome:?}"),
                    }
                }
                if let Some(failure) = receipt.boundary_failure() {
                    eprintln!("{failure}");
                }
                eprintln!("undelivered inputs: {:?}", receipt.undelivered_inputs());
                return Err(error.into()); // Inspect code() using the table below; do not retry blindly.
            }
            _ => (),
        }
    }
    Err(InvocationError::transport_closed().into())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let reply = call(DOCUMENT, r#"{"count":7}"#).await?;
    assert_eq!(reply, Reply { count: 7 });
    println!("count: {}", reply.count);
    Ok(())
}

#[cfg(test)]
mod tests;
