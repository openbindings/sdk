// Every receive lifecycle case also drives the provider through `Stream::poll_next`.
// This includes backpressure, half-close, cancellation, failed validation, and terminals.
async fn receive(
    input: &mut openbindings_invocation::ProviderInput,
) -> Option<openbindings::JsonValue> {
    std::future::poll_fn(|cx| futures_core::Stream::poll_next(std::pin::Pin::new(&mut *input), cx))
        .await
}
#[path = "support/lifecycle_cases.rs"]
mod lifecycle;
#[path = "support/operation_cases.rs"]
mod operation;
