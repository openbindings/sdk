# From a document to a typed result

An operation describes caller-facing input and output. A binding names one
realization and its source; the source's kind defines how an adapter interprets
source and binding data. The SDK ships no kind adapters. This example defines
`example.echo@1`: absent data, local execution, every input echoed unchanged.

The [external example crate](../examples/document-call/Cargo.toml) supplies all
dependencies. From the SDK checkout run:

```sh
cargo run --locked --manifest-path examples/document-call/Cargo.toml
```

For your own project, use the [first-use dependencies](rust-first-use.md), add
`openbindings-invocation` from the same checkout or archive set, and add Tokio
with `rt` and `macros`. Tokio here only drives the example; the SDK requires no
runtime. The complete `src/main.rs` below prints `count: 7`:

```rust
use openbindings::{Detail, Document, ExternalSchemas, JsonValue, ValueOutcome};
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
                        ValidationEvidence::Evaluation(outcome) => match outcome {
                            ValueOutcome::Fails {
                                problems,
                                problems_complete,
                                ..
                            } => {
                                eprintln!("fails; diagnostics complete: {problems_complete}");
                                for problem in problems {
                                    eprintln!(
                                        "instance={:?} code={} details={}",
                                        problem.instance.to_string(),
                                        problem.code,
                                        serde_json::to_string(&problem.details)?
                                    );
                                }
                            }
                            ValueOutcome::Undefined { detail } => {
                                eprintln!("undefined: {}", detail.code);
                            }
                            ValueOutcome::NoVerdict { detail } => {
                                eprintln!("no verdict: {}", detail.code);
                            }
                            ValueOutcome::Satisfies => eprintln!("satisfies"),
                        },
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
```

`prepare` resolves the selected operation and prepares its declared input and
output schemas without selecting a binding or running the adapter. Keep the
prepared owner for reuse; clone it for each attempt when its validator is
cloneable. `BindingChoice::Sole` requires exactly one supported binding.
`BindingChoice::Binding("echo.local".into())` would address that binding directly,
but currently still requires an adapter implementing `InvocationSupport`.
Input is checked before provider delivery and output before caller delivery.
Only `Satisfies` passes a declared schema gate. An absent schema adds no check.

The example handles JSON admission, unsupported version, document nonconformance
or undetermined conformance, missing operation, all four preparation-failure
kinds, request admission, send failure, unsuccessful terminal and typed decoding
separately. `Document::parse` returns syntax errors; use `validate_document`
instead when syntax itself should appear as an OBI-01 report finding.
For a streaming writer, continue reading the terminal even if a send fails;
the terminal and local control carry the attempt's authoritative outcome.

Use `InvocationError::code()` to decide how your application handles a terminal:

| Code | Caller action and meaning |
| --- | --- |
| `ERR_OPERATION_NOT_FOUND`, `ERR_BINDING_NOT_FOUND` | Correct the address or available candidates; resolution failed before provider dispatch. |
| `ERR_BINDING_SELECTION_REQUIRED` | Choose a binding or an ordered list; multiple candidates remained. |
| `ERR_OPERATION_VALIDATION_FAILED` | Inspect local direction and `Fails` evidence; that value was withheld. Earlier values or effects may exist. |
| `ERR_SCHEMA_UNRESOLVED` | Inspect preparation, `Undefined` or `NoVerdict` evidence. No satisfaction was established; fix resources/capability/limits as appropriate. |
| `CONTEXT_REQUIRED` | Inspect `context_required()` and obtain approved context through the host before a fresh attempt. The provider asserts no output or observable operation effect. |
| `ERR_REFUSED` | Respect refusal; the provider asserts no observable operation effect. |
| `ERR_CANCELLED` | Treat the attempt as cancelled; cancellation promises no rollback or remote termination. |
| `ERR_EXECUTION_FAILED` | Inspect local boundary evidence if present; dispatch and effects are otherwise unspecified. |
| `ERR_TRANSPORT_CLOSED`, `ERR_FRAME_PROTOCOL` | Treat the exchange as failed; effects may already exist. |
| Other codes | Follow the governing kind's meaning; preserve the code and optional exact data without guessing retry safety. |

A provider can also report `ERR_BINDING_NOT_FOUND`; the SDK only establishes the
before-dispatch guarantee for its own resolution failures. No error automatically
retries. Use a fresh cancellation token per attempt; pass context when needed.

A successful output is not terminal success. This bounded echo example waits for
`Complete`, checks delivery and cardinality, then calls `serde_json::from_str` on
the exact output text. A `count` of 70000 satisfies the integer schema but cannot
fit `Reply::count`, so decoding still fails. Choose types that represent your
contract; use exact `JsonValue` fields when ordinary Rust types cannot preserve it.
For ongoing streams, decode each value while continuing to observe the terminal.

TypeScript has no invocation surface; use its [document validation](../packages/typescript/README.md#first-useful-result) and [value evaluation](../packages/typescript/README.md#evaluate-values) guides.

The [invocation guide](invocation.md) covers streaming, cancellation and context
in more detail. [Host support](support.md) explains isolation for expensive work.
`tools/verify-rust-snippets.py` checks this fence against the external crate,
runs its failure tests and lint, and compiles and executes the verbatim program;
the native CI verifier includes that check.
