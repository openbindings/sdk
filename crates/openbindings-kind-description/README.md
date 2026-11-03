# OpenBindings kind description

Exact carriers and explicit provider clients for the kind-descriptor interface:
listed kinds, kind descriptions, check disclosures, proposed sources and checked
source entries. Support is a list of operation keys claimed by the provider for
that kind. An absent operation key makes no claim.

Constructors check carrier shape and retain original JSON. Allowed unknown output
members and extensions survive; absent data differs from present null. Data
schemas remain exact object/boolean values. These carriers prove neither document
conformance nor JSON Schema validity, availability, trust or permission to execute.
Applications inspect `CheckDescription::executes` before choosing to perform a
check. Context is supplied separately from the source entry authored afterward.

`KindDescriptor` provides the two capability queries; `SourceChecker` provides
source checking. Their borrowed futures require no executor, `Send` bound or
implicit network access. `KindClient` and `SourceCheckClient` apply finite carrier
handoff limits, exact request/response kind matching and cooperative cancellation.
Providers own admission of their outputs. A transport, decoding or admission
failure remains `CheckFailure::Operational`; a kind's rejection carries its own
code, readable message and optional exact data. `ContextRequired` carries the
shared context challenge. The SDK never acquires context or retries a check.
Native errors are not serialized or measured as JSON carriers.

```rust
use openbindings::JsonValue;
use openbindings_kind_description::{
    CancellationToken, CheckFailure, CheckSourceRequest, CheckedSource,
    ClientError, SourceCheckClient, SourceChecker,
};

async fn check_explicit_proposal<P: SourceChecker>(
    provider: &P,
    control: CancellationToken,
) -> Result<CheckedSource, ClientError<CheckFailure<P::Error>>> {
    // The application has already chosen to run the kind's disclosed check.
    let proposal = CheckSourceRequest::from_json(
        JsonValue::parse(r#"{"kind":"example.embedded@1","data":null}"#).unwrap(),
    ).unwrap();
    SourceCheckClient::new(provider).check_source(proposal, control).await
}
```

Calls are lazy: dropping an unpolled future does not invoke a provider. Cancellation
before dispatch prevents it. While a call is pending, caller cancellation reaches
the provider's attempt token and wakes the client. Dropping work cancels that
attempt before releasing the provider future; it never cancels the caller token.
Cancellation during provider cleanup takes precedence over handing off a result.
The provider must cooperate to stop external work; the SDK does not claim to undo
an effect the provider has already performed.

This unpublished companion applies interface version `0.1.0` at revision
`5815fb106bddd2c85ff4e93e0077570b3f6b9e74`, alongside the OpenBindings 0.2 document
model. Interface and SDK versions are independent. No TypeScript role API is
claimed.
