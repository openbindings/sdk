# OpenBindings invocation

Invoke a realization through a binding, or invoke an operation through its
conformant interface document. Implement `binding::BindingInvoker` for a kind;
`binding::Invoker` manages input, output and the terminal result. An operation
invoker adds document selection and declared value-contract checks.

```rust
use openbindings_invocation::CancellationToken;
use openbindings::{JsonValue, Source};
use openbindings_invocation::{
    binding::{BindingInvoker, Invoker},
    BindingRequest, BindingValue, InvocationError, ProviderIo,
};
use std::{future::Future, pin::Pin};

struct Echo<'a> { kind: &'a str }
impl BindingInvoker for Echo<'_> {
    type Invocation<'a> =
        Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
        where Self: 'a;

    fn supports_kind(&self, kind: &str) -> bool { kind == self.kind }

    fn invoke<'a>(
        &'a self, _request: BindingRequest<'a>, mut io: ProviderIo,
        _control: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            while let Some(value) = io.next().await {
                io.emit(value).await?;
            }
            Ok(())
        })
    }
}

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let kind = String::from("urn:example:echo:v1");
let invoker = Invoker::new(Echo { kind: &kind });
let source = Source { kind: kind.clone(), ..Default::default() };
let binding = BindingValue::default(); // A by-value binding may be {}.
let request = BindingRequest { source: &source, binding: &binding, context: None };
let (mut input, mut output) = invoker.invoke(request).split();

let send = async {
    input.write(JsonValue::parse("9007199254740993").unwrap()).await?;
    input.close();
    Ok::<_, openbindings_invocation::WriteError>(())
};
let receive = async {
    let mut count = 0;
    while let Some(value) = output.next().await {
        let value = value?; // Observe a failure even after earlier output.
        assert_eq!(value.text(), "9007199254740993");
        count += 1;
    }
    assert_eq!(count, 1); // Successful exhaustion is distinct from one JSON null.
    Ok::<_, InvocationError>(())
};
// Poll input and output concurrently with the application runtime.
let (write_result, terminal_result) = tokio::join!(send, receive);
terminal_result?; // Preserve the authoritative output failure if writing closed.
write_result?;
# Ok(())
# }
```

Providers can split `ProviderIo` into `ProviderInput` and `ProviderOutput`.
`ProviderInput` implements `futures_core::Stream<Item = JsonValue>` for manually
polled state machines and retains its inherent async `next` and `close` methods.
Both receive paths preserve backpressure, input order and cancellation. A failed
input value contract suspends the provider until its job is dropped, so validation
failure never appears as clean end of input.

The same input/output lifetime supports empty, unary and streaming interactions.
For document calls, create `operation::Invoker` with a `Conformant<Document>` and
provider, select an evaluator with `with_evaluator`, hold it in an `Arc`, then
`prepare` an operation and invoke an `Attempt`. The [invocation guide](https://github.com/openbindings/sdk/blob/v0.2.0-alpha.1/docs/invocation.md)
shows the complete caller and describes selection, limits and cancellation.

## Limits

Read output through its terminal result. A first output value does not establish
success; a later failure is still authoritative. Close input when there are no
more values. Poll both halves concurrently for bidirectional work.

Without an evaluator, `operation::Invoker::prepare` refuses any stated value
contract, even `true`; absent value contracts need no evaluator. Plain `invoke`
prepares each direction lazily on its first value, so an output refusal can follow
provider effects. Kind interpretation and transport belong to the provider.
Preflight is advisory and separate from invocation: both facades' `preflight`
run the SDK's checks for every provider and ask the provider's hook when it
overrides `BindingInvoker::preflight`; by default there is none. Construction
allocates nothing; `prepare` is called on an invoker held in an `Arc`, and the
prepared operation keeps a share of it, so an `Arc` of one handle can serve
spawned tasks.

Providers may borrow configuration and return local futures. Cancellation is
cooperative; it cannot preempt a blocking poll. This SDK's carrier admission,
bounded queues and terminal arbitration are documented policies beyond the
shared interface shapes.

This provisional Rust API uses Binding Invoker and Operation Invoker 0.1.0 with
OpenBindings 0.2 carriers. It requires Rust 1.88. Public TypeScript invocation is
not provided.

## Reference

See the `binding` and `operation` modules and the
[complete consumer](https://github.com/openbindings/sdk/tree/v0.2.0-alpha.1/examples/binding-invocation-consumer).
