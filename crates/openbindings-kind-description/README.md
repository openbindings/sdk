# OpenBindings kind description

For a complete protocol implementation walkthrough, read
[writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md).

Exact carriers and directly callable implementations of the kind-descriptor interface:
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
source checking. Their borrowed futures require no async runtime, `Send` bound or
implicit network access. Implement `handle_list_kinds`, `handle_describe_kind` and
`handle_check_source` as appropriate. Import `KindDescriptorExt` or `SourceCheckerExt`
to call the checked `list_kinds`, `describe_kind` and `check_source` methods.
Their SDK-owned blanket implementations apply carrier handoff limits, exact
request/response kind matching and cooperative cancellation. The corresponding
`*_with_limits` methods take caller-selected `DescriptionLimits` as the last argument.
Explicit `handle_*` calls bypass SDK checks. Local checked failures are `KindError`.
Providers own admission of their outputs. A transport, decoding or admission
failure remains `CheckFailure::Operational`; a kind's rejection carries its own
code, readable message and optional exact data. `ContextRequired` carries the
shared context challenge. The SDK never acquires context or retries a check.
Native errors are not serialized or measured as JSON carriers. `Display` includes
the provider's own error message, or a rejection's code and message; `Debug` keeps
payloads hidden. For native errors implementing `std::error::Error + 'static`,
`KindError::source()` and `CheckFailure::source()` retain the underlying cause.
Choose where to display these messages according to the application's disclosure
policy.

```rust
use openbindings::JsonValue;
use openbindings_kind_description::{
    CancellationToken, CheckFailure, CheckSourceRequest, CheckedSource,
    KindError, SourceCheckerExt, SourceChecker,
};

async fn check_explicit_proposal<P: SourceChecker>(
    provider: &P,
    control: CancellationToken,
) -> Result<CheckedSource, KindError<CheckFailure<P::Error>>> {
    // The application has already chosen to run the kind's disclosed check.
    let proposal = CheckSourceRequest::from_json(
        JsonValue::parse(r#"{"kind":"example.embedded@1","data":null}"#).unwrap(),
    ).unwrap();
    provider.check_source(proposal, control).await
}
```

Calls are lazy: dropping an unpolled future does not invoke a provider. Cancellation
before dispatch prevents it. While a call is pending, caller cancellation reaches
the provider's attempt token and wakes the caller. Dropping work cancels that
attempt before releasing the provider future; it never cancels the caller token.
Cancellation during provider cleanup takes precedence over handing off a result.
On cancellation the provider future is dropped and never polled again, so async
cleanup cannot run inside it afterward. Synchronous `Drop` can release held
resources. The attempt token is useful for detached work and synchronous loops
that stop cooperatively. The SDK does not undo effects already performed.

This unpublished companion applies interface version `0.1.0` at revision
`3e3dc453a93b611a8e9ea1813ec63a2216830f45`, alongside the OpenBindings 0.2 document
model. Interface and SDK versions are independent. No TypeScript interface API is
claimed.

Descriptions carry `pull` exactly when `supports` names
`openbindings.source-puller.pullSource`, and `invoke` exactly when it names
`openbindings.binding-invoker.invokeBinding`. `PullDescription` and
`InvokeDescription` retain each disclosure's sentence and `executes` flag.
Unknown operation keys in `supports` remain valid claims. These disclosures do
not grant permission to execute.
