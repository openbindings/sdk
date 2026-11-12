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

Use `DescribeKindRequest::new(kind)` or
`CheckSourceRequest::new(kind, data, context)` to construct requests from typed
values. Both apply the same carrier checks as `from_json`, escape the kind name,
and use default exact-JSON admission limits. Data and context retain their exact
JSON, including duplicate nested members and number spellings.

To evaluate proposed data against `KindDescription::source_data()` or
`binding_data()`, supply that exact `JsonValue` as an `openbindings::ExternalSchema`
under a URI the caller chooses, build `ExternalSchemas`, then call
`resources.prepare(uri, &evaluator)`. The URI is the resource's retrieval identity
for relative references and diagnostics; its own `$id` governs the reference base
when present. Supply referenced resources in the same set. The resource is its own
schema root, so `#/$defs/...` and `{"$ref":"#"}` stay inside it. Preparation failure is
separate from every data value outcome. This uses the same evaluator seam as
contained operation schemas and needs no synthetic OBI or core `Schema`.

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
Native errors are not serialized or measured as JSON carriers. `Display` prints
each SDK error's own layer and `Debug` keeps payloads hidden. Read a rejection's
`code()` and `message()` explicitly. For native errors implementing
`std::error::Error + 'static`, `KindError::source()` and `CheckFailure::source()`
retain the underlying cause. Apply the application's disclosure policy before
printing a native cause or a cause chain.

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
    let proposal = CheckSourceRequest::new(
        "example.embedded@1", Some(JsonValue::null()), None,
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
`67f707c31aecbdf0551385e369d8024097db6278`, alongside the OpenBindings 0.2 document
model. Interface and SDK versions are independent. No TypeScript interface API is
claimed.

Descriptions carry `pull` exactly when `supports` names
`openbindings.source-puller.pullSource`, and `invoke` exactly when it names
`openbindings.binding-invoker.invokeBinding`. `PullDescription` and
`InvokeDescription` retain each disclosure's sentence and `executes` flag.
Unknown operation keys in `supports` remain valid claims. These disclosures do
not grant permission to execute.

Borrow an admitted carrier's exact object with `value()`; use `value().clone()`
when an owned object is needed. On interface carriers, `to_json` means bounded
encoding, as on invocation errors and requests.

`KindDescription::from_json` requires `check`, `pull` and `invoke` to be present
exactly when `supports` names `openbindings.kind-descriptor.checkSource`,
`openbindings.source-puller.pullSource` and
`openbindings.binding-invoker.invokeBinding`, respectively. A vendor operation
key alone does not admit a `check` disclosure.
