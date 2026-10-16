// CA-4: run the same provider lifecycle assertions through the inherent receiver.
async fn receive(
    input: &mut openbindings_invocation::ProviderInput,
) -> Option<openbindings::JsonValue> {
    input.next().await
}
#[path = "support/lifecycle_cases.rs"]
mod cases;
