# Writing a binding kind adapter

OpenBindings describes operations in a JSON interface document. An operation
says what input and output values mean. A binding says how to reach one
realization of that operation. Each binding names a source, whose `kind`
identifies the rules for interpreting the source's and binding's `data`.
A kind can describe a protocol, an embedded catalog, or a fixed local inventory.
Its exact identifier, such as `example.memory@1`, is part of those rules.

An adapter crate is a Rust library that implements those rules using the SDK's
optional interface traits. It supplies protocol behavior. The application
constructs it with host configuration, chooses which binding to invoke or source
to pull, supplies runtime context, and decides which document edits to adopt.
The SDK supplies checked calls and exact JSON. Applications supply kind-specific
transports, credential storage and routing, and decide retries.

This page targets the current unpublished `0.2.0-alpha.1` SDK and its interface
revision `a3c19658fc89815d7a417eae29ba31fd4e930daf`. The document model is
OpenBindings 0.2. The [example crate](../examples/kind-adapter/README.md) contains
every Rust block below, with tests. Its fictional in-memory protocol needs no
network. It builds for native Rust and `wasm32-unknown-unknown`.

## Choose the interfaces

Implement only the interfaces your adapter supports. Invocation alone does not
require a descriptor, checker or puller. A catalog may describe kinds without
invoking anything. Each trait uses associated future types that can borrow the
adapter; none requires an async runtime, `Send` or `Sync`.

| Implement | Hooks | Application imports and calls |
| --- | --- | --- |
| `BindingInvoker` | `handle_invoke`, `handle_preflight` | `BindingInvokerExt`: `invoke`, `preflight` |
| `KindDescriptor` | `handle_list_kinds`, `handle_describe_kind` | `KindDescriptorExt`: `list_kinds`, `describe_kind` |
| `SourcePuller` | `handle_pull_source` | `SourcePullerExt`: `pull_source` |
| `SourceChecker` | `handle_check_source` | `SourceCheckerExt`: `check_source` |

The extension traits have SDK-owned blanket implementations. Applications use
them for admission, cancellation and result checks. Calling a hook directly
bypasses those checks. A router forwarding an already checked invocation to a
selected adapter calls its hook with the same `ProviderIo`, avoiding a second
invocation channel.

Every checked call also has a `*_with_limits` form, with limits as its last
argument. Invocation uses `InvocationLimits`, pull uses `PullLimits`, and
queries and source checks use `DescriptionLimits`. These bound SDK handoffs
and checks, not protocol work, transport buffering or all memory held by a call.
Choose separate bounds for those in the adapter and host. Parse incoming JSON
with explicit `JsonLimits` when the default admission policy is unsuitable;
limits on an already constructed request cannot undo its parsing allocations.

`DescriptionLimits` bounds request and response bytes and the listed-kind count.
`PullLimits` also bounds indexed entries and uses labels, relationship work and
relationship allocation reservations. Prospective core rebuilding uses the
original document's JSON limits. See the [pull limits](../crates/openbindings-source-pull/README.md)
for the current defaults and the scope of that accounting.

`invoke` and `preflight` return an immediate `Result` for local admission and a
lazy invocation or future for execution. Pull, description and source-check
calls return futures whose local checks also happen when polled. A missing
binding becomes `ERR_BINDING_NOT_FOUND` before the hook runs. A missing source
fails the checked pull before its hook runs.

`BindingInvoker` executes an explicitly addressed binding and does not validate
operation input or output schemas. Applications that want operation-name or
alias resolution, binding selection and per-value validation use
`DefaultOperationInvoker` with an explicit evaluator and `ExternalSchemas`.
Implement the separate `InvocationSupport::can_invoke` routing facet for that
composition. It is not a portable kind-support query. An explicit binding call
does not consult it. See [invocation](invocation.md) for the application side.

## Stage 1: define the kind and invoke it

Our kind is `example.memory@1`. Source data is exactly
`{"channel":"local"}`; binding data is exactly `{"mode":"echo"}`. It echoes
zero or more exact JSON input values, in order, until input closes. It offers
one operation, `echo`, with no declared input or output schema. The data refers
to nothing else in the document. There is no external transport or context
requirement. These are this fictional kind's rules.

Start an external library crate. The example's [manifest](../examples/kind-adapter/Cargo.toml)
uses version dependencies on `openbindings`, `openbindings-invocation`,
`openbindings-kind-description` and `openbindings-source-pull`, and an empty
`[workspace]` to keep it outside the SDK workspace. Packages are unpublished;
the repository verifier supplies local Cargo patches and verifies resolution
against the workspace lockfile. Do the same for an exact source checkout rather
than assuming a registry release has this API.

The following blocks build one `src/lib.rs` in order. The small construction
helpers use checked exact JSON composition. Their `expect` calls suit this
bounded demonstration. An adapter accepting arbitrary external content should
propagate parse, conversion and carrier admission failures through its native
error type instead of panicking.

<!-- guide:setup -->
```rust
use openbindings::JsonValue;
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, ContextChallenge, InvocationError,
    InvocationSupport, ProviderIo,
};
use std::{
    collections::BTreeMap,
    fmt,
    future::{Future, Ready, ready},
    pin::Pin,
};

pub const KIND: &str = "example.memory@1";

#[derive(Default)]
pub struct Memory;

pub struct AdapterError(pub &'static str);
impl fmt::Debug for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AdapterError { .. }")
    }
}
impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("kind adapter could not complete")
    }
}
impl std::error::Error for AdapterError {}

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).expect("valid example JSON")
}
fn object<const N: usize>(fields: [(&str, JsonValue); N]) -> JsonValue {
    // The checked serializer preserves nested exact values, including wide numbers.
    JsonValue::from_serializable(&BTreeMap::from(fields)).expect("bounded example object")
}
fn string(text: &str) -> JsonValue {
    JsonValue::string(text).expect("scalar string")
}
fn matches(data: Option<JsonValue>, expected: &str) -> bool {
    data.is_some_and(|value| value.semantic_eq(&json(expected)) == Some(true))
}
fn source_ok(source: &openbindings::Source) -> bool {
    source.kind() == KIND && matches(source.data(), r#"{"channel":"local"}"#)
}
```

A conformant document proves core fields and relationships. It does not prove
that an adapter understands the kind or its data. Inspect the binding, its
source and their data, and reject unsupported forms before doing work. The
binding invoker must not infer adaptation from an operation declaration. If a
kind needs operation schemas for adaptation, its binding data must express the
references to them.

<!-- guide:invoke -->
```rust
fn admit(request: &BindingRequest) -> Result<(), InvocationError> {
    let binding = request
        .interface
        .binding(&request.binding)
        .ok_or_else(InvocationError::binding_not_found)?;
    if !source_ok(&binding.source()) || !matches(binding.data(), r#"{"mode":"echo"}"#) {
        // This kind defines a fixed code and never copies rejected data into errors.
        return Err(InvocationError::new("example.memory.invalid-data", None).unwrap());
    }
    Ok(())
}
impl BindingInvoker for Memory {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;

    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            admit(&request)?;
            while let Some(value) = io.next().await {
                io.send(value).await?;
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        ready(admit(&request).map(|()| None))
    }
}
impl InvocationSupport for Memory {
    fn can_invoke(&self, kind: &str) -> bool {
        kind == KIND
    }
}
```

Returning `Ok(())` ends invocation successfully. Returning `Err` ends it
unsuccessfully. Do not send an extra terminal as an output value. This preflight
checks the addressed data and returns `None` (no prediction). Preflight must not consume
input, emit output, dispatch the requested operation or spend its approval.
A future invocation still resolves its own needs and can fail or ask for context.

## Stage 2: describe the kind

A descriptor tells applications which kinds it understands, which operations
it supports for each kind, and the schemas for source and binding data.
`supports` contains operation keys, not Rust trait names. The current keys for
invocation, pull and source check are
`openbindings.binding-invoker.invokeBinding`,
`openbindings.source-puller.pullSource` and
`openbindings.kind-descriptor.checkSource`. A descriptor with no source checker
makes no check claim and omits `check`.

For a provider that both routes and describes, under one configuration,
`can_invoke(kind)` must be true exactly for the kinds whose listing and
description include `openbindings.binding-invoker.invokeBinding`. This is
kind-level capability, not reachability or credentials, and it does not promise
that every binding succeeds. `can_invoke` is the local synchronous routing fact;
`supports` is the published claim. A descriptor is optional and its absence makes
no claim, so selection must not consult it. The SDK does not reconcile the two
answers for you; the test after the descriptor below checks them.

The listing and description below share their support list. `pull` and `invoke`
disclosures are present exactly when their operation keys are supported.
Disclosures describe what happens and whether it executes something the source
or binding data names. The application decides whether to authorize execution.
Listing and describing perform no operation under the kind.

<!-- guide:describe -->
```rust
use openbindings_kind_description::{
    DescribeKindRequest, KindDescription, KindDescriptor, KindSummary,
};

const SUPPORTS: &str = r#"[
    "openbindings.binding-invoker.invokeBinding",
    "openbindings.source-puller.pullSource"
]"#;

impl KindDescriptor for Memory {
    type Error = AdapterError;
    type List<'a> = Ready<Result<Vec<KindSummary>, Self::Error>>;
    type Describe<'a> = Ready<Result<KindDescription, Self::Error>>;

    fn handle_list_kinds(&self, _: CancellationToken) -> Self::List<'_> {
        ready(Ok(vec![
            KindSummary::from_json(object([
                ("kind", string(KIND)),
                ("supports", json(SUPPORTS)),
            ]))
            .unwrap(),
        ]))
    }
    fn handle_describe_kind(
        &self,
        request: DescribeKindRequest,
        _: CancellationToken,
    ) -> Self::Describe<'_> {
        if request.kind().as_str() != Some(KIND) {
            return ready(Err(AdapterError("unsupported kind")));
        }
        ready(Ok(KindDescription::from_json(object([
            ("kind", string(KIND)),
            ("supports", json(SUPPORTS)),
            ("description", string("Echo exact JSON through a local channel.")),
            ("invoke", json(r#"{"description":"Echoes each input until input closes.","executes":false}"#)),
            ("pull", json(r#"{"description":"Reads a fixed local inventory.","executes":false}"#)),
            ("sourceData", json(r#"{"type":"object","properties":{"channel":{"const":"local"}},"required":["channel"],"additionalProperties":false}"#)),
            ("bindingData", json(r#"{"type":"object","properties":{"mode":{"const":"echo"}},"required":["mode"],"additionalProperties":false}"#)),
        ])).unwrap()))
    }
}
```

Check that agreement through the public APIs: list the kinds, describe each,
and compare both claims with `can_invoke`, including a kind the adapter does not
know. Keep the known routing inventory in the test so a listing that omits a kind
cannot silently remove coverage.

<!-- guide:agreement -->
```rust
#[tokio::test]
async fn routing_agrees_with_published_invocation_support() {
    use kind_adapter::{KIND, Memory};
    use openbindings_invocation::{CancellationToken, InvocationSupport};
    use openbindings_kind_description::{DescribeKindRequest, KindDescriptorExt};

    const INVOKE: &str = "openbindings.binding-invoker.invokeBinding";
    let adapter = Memory; // One configuration for all three queries.
    let kinds = adapter.list_kinds(CancellationToken::new()).await.unwrap();
    // Include the adapter's known routing kind even if a broken listing omits it,
    // and a negative case outside its inventory.
    for kind in kinds
        .iter()
        .map(|summary| summary.kind().as_str().unwrap())
        .chain([KIND, "example.unknown@1"])
    {
        let listed = kinds.iter().any(|summary| {
            summary.kind().as_str() == Some(kind)
                && summary.supports().any(|operation| operation == INVOKE)
        });
        let description = adapter
            .describe_kind(
                DescribeKindRequest::new(kind).unwrap(),
                CancellationToken::new(),
            )
            .await;
        let described = match description {
            Ok(description) => {
                assert_eq!(description.kind().as_str(), Some(kind));
                description.supports().any(|operation| operation == INVOKE)
            }
            Err(_) => {
                assert_eq!(kind, "example.unknown@1");
                false
            }
        };
        assert_eq!(adapter.can_invoke(kind), listed);
        assert_eq!(adapter.can_invoke(kind), described);
    }
}
```

Data schemas must actually describe the forms the kind accepts. Carrier
constructors check that these schemas are objects or booleans, but do not prove
JSON Schema validity. For example, an object containing `"type":5` is not
rejected on that basis. Test descriptions against your own admission rules.
Omitted `data` and explicit JSON null are different; this kind accepts neither.

Applications can check proposed data with the usual evaluator seam: supply a
description's `source_data()` or `binding_data()` value as an `ExternalSchema`
under a URI the caller chooses, build `ExternalSchemas`, then call
`resources.prepare(uri, &evaluator)`. That URI is the resource's retrieval identity
for relative references and diagnostics; its own `$id` governs when present.
Put referenced resources in the same set. Preparing the resource as its own root
preserves its `#/$defs/...` and `{"$ref":"#"}` meaning. Handle preparation failures
separately from data value outcomes. This does not construct a core `Schema` or
require a synthetic OBI.

`SourceChecker` is optional and is not implemented here. Its hook accepts a
`CheckSourceRequest` containing `{kind, data?, context?}` and returns a
`CheckedSource`, or `CheckFailure`. A successful checked source is the source
entry the kind stands behind, possibly completed or normalized as its rules
allow. It excludes runtime context. It changes no document and performs none
of the source's operations. Disclose any execution needed to check it before
the application calls `check_source`. The SDK checks carrier size and matching
request/response kinds; availability, trust and kind-specific validity remain
the kind's and application's responsibilities.

## Stage 3: pull the source

Pull observes one source and projects what it offers into additions plus a
report against the original conformant document. It does not invoke the source's
operations. A kind can read embedded source data or use host transport to read
what the data names. Disclose execution needed to read the source with
`pull.executes`.

This small implementation compares absent or Boolean schema slots: absent
equals absent and differs from a Boolean schema. It leaves object-schema
comparison unsupported because profile normalization can fail, even when the
other slot is absent. It resolves `echo` through aliases, reports the existing
operation's primary key on reuse, and reports a conflict
when that operation has Boolean schemas. Object schemas instead produce an
implementation-unsupported omission or an unassessed existing entry. It accounts
for each existing binding of the selected source. A recognized binding with
added Boolean operation schemas is stale
and gets a removal proposal. Unrecognized binding data is unmatched.

<!-- guide:pull -->
```rust
use openbindings_source_pull::{PullFailure, PullRequest, PullResult, SourcePuller};

impl SourcePuller for Memory {
    type Error = AdapterError;
    type Pull<'a> = Ready<Result<PullResult, PullFailure<Self::Error>>>;

    fn handle_pull_source(&self, request: PullRequest, _: CancellationToken) -> Self::Pull<'_> {
        ready(pull(&request).map_err(PullFailure::Operational))
    }
}

fn comparable(operation: &openbindings::Operation) -> bool {
    // Full profile normalization can fail even when the offered slot is absent.
    // This small adapter compares only absent or Boolean schemas.
    operation
        .input()
        .into_iter()
        .chain(operation.output())
        .all(|schema| schema.json().view().as_bool().is_some())
}

fn pull(request: &PullRequest) -> Result<PullResult, AdapterError> {
    let interface = request.interface();
    let source = interface
        .source(request.source())
        .ok_or(AdapterError("missing source"))?;
    if !source_ok(&source) {
        return Err(AdapterError("unsupported kind or source data"));
    }
    let mut existing = Vec::new();
    let mut bound = false;
    for binding in interface.bindings().into_iter().flatten() {
        if binding.source_key() != request.source() {
            continue;
        }
        let recognized = matches(binding.data(), r#"{"mode":"echo"}"#);
        bound |= recognized;
        let operation = binding.operation();
        let mut remove = Vec::new();
        if operation.input().is_some() {
            remove.push(string("input"));
        }
        if operation.output().is_some() {
            remove.push(string("output"));
        }
        let mut entry = BTreeMap::from([("binding", string(binding.key()))]);
        let status = if !recognized {
            "unmatched"
        } else if !comparable(&operation) {
            entry.insert("reasonCode", string("example.memory.object-schema"));
            entry.insert(
                "message",
                string("This adapter does not compare object schemas."),
            );
            "unassessed"
        } else if remove.is_empty() {
            "current"
        } else {
            // This fixed protocol offers no schemas. Removal is an explicit proposal.
            entry.insert("change", string("dangerous"));
            entry.insert(
                "proposal",
                object([("remove", JsonValue::from_serializable(&remove).unwrap())]),
            );
            "stale"
        };
        entry.insert("status", string(status));
        existing.push(JsonValue::from_serializable(&entry).unwrap());
    }
    let mut additions = json("{}");
    let mut added = Vec::new();
    let mut omitted = Vec::new();
    if !bound {
        let operation = interface.resolve_operation("echo");
        if let Some(operation) = &operation
            && !comparable(operation)
        {
            omitted.push(object([
                ("status", string("implementation-unsupported")),
                ("reasonCode", string("example.memory.object-schema")),
                (
                    "message",
                    string("This adapter does not compare object schemas."),
                ),
                ("item", string("operations/echo")),
                (
                    "existing",
                    string(&format!("operations/{}", operation.key())),
                ),
            ]));
        } else if let Some(operation) = &operation
            && (operation.input().is_some() || operation.output().is_some())
        {
            // Absent schemas differ from present schemas, even a Boolean true schema.
            omitted.push(object([
                ("status", string("conflict")),
                ("reasonCode", string("example.memory.schema-conflict")),
                (
                    "message",
                    string("The existing operation declares schemas."),
                ),
                ("item", string("operations/echo")),
                (
                    "existing",
                    string(&format!("operations/{}", operation.key())),
                ),
                ("offered", json("{}")),
            ]));
        } else {
            let key = format!("echo.{}", request.source());
            if interface.binding(&key).is_some() {
                return Err(AdapterError("generated binding key is occupied"));
            }
            let operation_key = operation.as_ref().map_or("echo", |value| value.key());
            let binding = object([
                ("operation", string(operation_key)),
                ("source", string(request.source())),
                ("data", json(r#"{"mode":"echo"}"#)),
            ]);
            let bindings = object([(&key, binding)]);
            additions = if operation.is_some() {
                object([("bindings", bindings)])
            } else {
                object([
                    ("operations", json(r#"{"echo":{}}"#)),
                    ("bindings", bindings),
                ])
            };
            let mut entry = BTreeMap::from([
                ("binding", string(&key)),
                ("operation", string(operation_key)),
            ]);
            if operation.is_some() {
                entry.insert("reused", json("true"));
            }
            added.push(JsonValue::from_serializable(&entry).unwrap());
        }
    }
    PullResult::from_json(object([
        ("additions", additions),
        (
            "report",
            object([
                ("exhaustive", json("true")),
                ("added", JsonValue::from_serializable(&added).unwrap()),
                ("existing", JsonValue::from_serializable(&existing).unwrap()),
                ("omitted", JsonValue::from_serializable(&omitted).unwrap()),
            ]),
        ),
    ]))
    .map_err(|_| AdapterError("invalid generated pull result"))
}
```

This example fails the pull with a native error if its generated binding key
is occupied. A larger adapter needs a naming policy that can represent such
cases. The fixed operation name comes from the source's inventory, never from
the destination document. Core `is_valid_name` checks names before you author
them; a protocol method such as `math/add` is not a valid operation key. Define
a stable mapping from protocol names and retain the protocol address in binding
data. See the [larger pull example](../examples/source-pull-consumer/README.md)
for nontrivial schema comparison and reconciliation.

## Source data, binding data and exact values

Source data describes the common source, such as an embedded catalog or an
address. Binding data describes the realization of one operation within that
source, such as its target and adaptation rules. Only the kind defines their
meaning. Core checks their JSON and surrounding document, not kind semantics.
No automatic source check or data-schema evaluation happens before invocation
or pull. Validate what you need in each hook, sharing that logic where useful.

Keep exact values as `JsonValue`. `JsonValue::parse` preserves number tokens,
including integers wider than binary64. `JsonValue::from_serializable` now
supports nested exact values in maps, arrays and serializable structs; the
`object` helper above uses that route. It checks unique members and limits,
with a recursive conversion ceiling of 128 containers. The exact parser has
separate limits. Avoid converting exact values through `serde_json::Value` or
`serde_json::json!`, which can lose wide-number precision. When working with
already encoded JSON, parse those bytes directly.

Secrets never go in documents or diagnostics. Keep credentials and approvals
in separately supplied runtime `Context`, or host configuration with a clear
lifetime. Do not copy context into source data, binding data, reports,
descriptions, messages, error codes or error data. Redacted SDK `Debug` output
does not sanitize your own logs or serialization.

## Report failure at the right boundary

Invocation returns `InvocationError`: a nonempty code and optional portable
application data. `InvocationError::new` preserves unknown codes and distinguishes
absent data from JSON null. Define stable, fixed kind-specific codes for wrong
kind or unreadable data if the kind needs them. There is no universal wrong-kind
or invalid-kind-data code. This example uses `example.memory.invalid-data`.
Use `ERR_EXECUTION_FAILED` when no more specific governed meaning applies.

| Code | Adapter obligation |
| --- | --- |
| `CONTEXT_REQUIRED` | Supply a valid challenge, before every output and every observable effect of the requested operation. |
| `ERR_REFUSED` | Assert no observable operation effect occurred. Use `InvocationError::refused()` only when that is true. |
| `ERR_CANCELLED` | Report caller cancellation, without promising rollback or remote termination. |
| `ERR_EXECUTION_FAILED` | Report failure with no cause, retry or dispatch-state claim. |
| `ERR_TRANSPORT_CLOSED` | The outer invocation transport ended before its terminal arrived. An ordinary underlying protocol failure does not automatically have this meaning. |
| `ERR_FRAME_PROTOCOL` | Report a malformed or invalid invocation frame sequence. |
| `ERR_BINDING_NOT_FOUND` | The named binding is absent; the checked call handles this. |

A binding adapter must not return the operation-invoker-owned codes
`ERR_OPERATION_NOT_FOUND`, `ERR_BINDING_SELECTION_REQUIRED`,
`ERR_OPERATION_VALIDATION_FAILED` or `ERR_SCHEMA_UNRESOLVED`. Checked invocation
replaces these with `ERR_EXECUTION_FAILED` and records a local boundary finding.
The SDK cannot prove a no-effects claim. It also does not reject an adapter's
unsupported claim of cancellation, so test that your mapping is truthful.

Use `ContextChallenge::try_from` on an exact `{message, schema}` object and
`InvocationError::challenge` for missing runtime context. The schema is an
object-form schema for the next attempt's context; mark secret properties
`writeOnly`. The challenge is a request, not permission to release credentials.
The application acquires values, decides whether to disclose them and starts a
new attempt. The SDK never acquires context or retries for you.

A provider that makes no prediction can use the one-line body
`ready(Ok(None))` with `Ready<Result<Option<ContextChallenge>, InvocationError>>`.
The [binding-invoker interface](https://github.com/openbindings/interfaces/blob/a3c19658fc89815d7a417eae29ba31fd4e930daf/binding-invoker/README.md)
states that `null` is always conformant, even when invocation will need context.
`None` means no prediction; it does not promise that invocation will proceed.

Preflight returns `Ok(Some(challenge))` when it predicts
context requirements. Returning a `CONTEXT_REQUIRED` error there is invalid and
becomes `ERR_EXECUTION_FAILED`. Checked invocation similarly rejects a challenge
after output. The adapter must enforce the before-effects guarantee itself.

Native transport errors, exception text, stacks, headers, protocol envelopes and
raw debugging evidence stay outside portable invocation error data unless a
governing rule explicitly defines them as application data. Use fixed safe codes;
keep native evidence within host-controlled diagnostics, excluding secrets.
`InvocationError` has no portable message field. Its `Display` and `Debug` print
only governed codes by name and hide unknown codes and all data. Read `code()`
explicitly for a provider-defined code. Do not embed user values in codes.

Pull uses `PullFailure::Operational(native_error)`,
`PullFailure::ContextRequired(challenge)` or `PullFailure::Cancelled`.
Queries use their associated native error type. Source checking distinguishes
`CheckFailure::Rejected(CheckRejection)` with code, readable message and optional
data from `Operational`, `ContextRequired` and `Cancelled`. A rejection is not
a context challenge. `PullError` and `KindError` distinguish these outcomes from
local SDK validation failures. SDK wrapper `Display` prints its own layer and
exposes a native error through `source()` when it implements the corresponding
Rust trait. `Debug` hides payloads. Apply the application's disclosure policy
before printing native errors or chains; native formatting is provider-owned.

## Streaming output and input

`ProviderIo::send(value).await` emits one output; `next().await` receives one
input. Echo uses both repeatedly. A protocol can emit many outputs for one
input, consume many inputs, or close its input while continuing output. Use
`close_input()` to decline more input. `ProviderIo::split` gives independent
`ProviderInput` and `ProviderOutput` handles when the directions need separate
polling; the input handle also implements `futures_core::Stream`.

Always await sends and respect backpressure. Do not collect an unbounded result
stream before sending it. A pending send dropped before commitment does not
commit its value. Late failure leaves already delivered outputs in place. Only
the terminal `Complete` establishes success; `Error` establishes failure.

On the application side, only the output handle drives adapter and validation
work. Poll input submission and output consumption concurrently. Awaiting a send
while leaving output unpolled can stall the call. Closing input drains accepted
values; dropping output cancels the attempt. A successful input send means queue
acceptance, not processing. `InvocationControl::undelivered_inputs()` reports
accepted inputs never received by the adapter after settlement.

Default `InvocationLimits` allow one queued input, four queued outputs and
64 KiB for an encoded terminal error or advisory challenge. `max_value_bytes`
is `None` by default, so no extra size cap applies to already parsed values.
Set it explicitly when needed. An oversized error becomes
`ERR_EXECUTION_FAILED`; retain `call.control()` or `output.control()` to inspect
`boundary_failure()`. Validation evidence is separately available through
`validation_failure()`. These local findings are not portable error payloads.

## Cancellation and cleanup

When a checked call observes cancellation, the adapter future is dropped and
never polled again. The SDK owns that race; the hook does not need to race its
token around each await. Async cleanup inside that future cannot run afterward.
Do not await `control.cancelled()` and then expect to await a cancel message or
flush a transport there. Synchronous `Drop` can release resources held by the
future. Dropping invocation output also releases the driver and adapter future.
Pull, description, check and preflight calls follow the same future-drop model.

The token passed to a hook belongs to that attempt. Invocation, preflight, pull,
description and source-check cancellation revoke it before releasing pending
provider work. None of these drops cancels the caller's token. The token
is useful for independently polled or detached work and for synchronous loops
that periodically test `is_cancelled()`. Those loops must still yield if the
host needs other tasks to run. The token cannot interrupt arbitrary synchronous
code, undo effects or guarantee remote cancellation. If a protocol requires an
async cancellation exchange, arrange host-owned work whose lifetime outlives
the dropped attempt and define its own cleanup policy.

## Send and Wasm hosts

The interface traits impose no `Send` or `Sync` requirement on adapters, errors
or associated futures. This example chooses a boxed `+ Send` invocation future.
Concrete checked calls preserve `Send` when their captured adapter, future and
other state allow it. A thread executor may additionally require `Send` and
`'static`; satisfy those through ownership and composition, not a new SDK bound.

A local adapter can hold `Rc`, browser handles or other `!Send` state and use
`Pin<Box<dyn Future<Output = ...> + 'a>>`, without `+ Send`, for its associated
future. Poll it on a local executor. That choice erases `Send` even when a
particular native transport happens to be thread-safe. There are three useful
choices: always box with `Send` when supported; always use a local box; or expose
a named concrete future type that retains conditional `Send` from its fields.
The associated future slot cannot use unstable `impl Trait` in an associated
type on this SDK's stable Rust baseline.

`wasm32-unknown-unknown` does not itself require a `!Send` adapter. The plain
in-memory example keeps its `+ Send` future there. A browser or Workers transport
can use local futures for its host handles. Supply that transport through adapter
configuration, for example a generic transport type or host callback that
returns a local future. The adapter translates protocol requests and values;
the host performs permitted I/O. Assume no threads, native sockets or file
system. The SDK chooses no host transport or task-spawning mechanism.

A Wasm build alone does not establish that an artifact will instantiate in a
particular host. Dependencies can introduce wasm-bindgen imports. A browser
integration should supply its generated glue; a Workers integration needs a
compatible module and host imports. Inspect imports and test the actual host.
This example's check builds Wasm but does not run browser or Workers transport.

## Pull reports beyond the example

A `PullResult` must contain `additions` and `report`. Each additions map
(`operations`, `schemas`, `bindings`) is optional. The report requires
`exhaustive` and `added`; `existing` and `omitted` are optional. A non-exhaustive
report requires a `limitation` with nonempty `code` and `message`; an exhaustive
one must omit it.

Every added key must be new in its map. Operation keys must also avoid existing
aliases. Added bindings name the selected source. Each has exactly one `added`
report entry naming its binding and operation by primary key. `reused: true`
means that operation existed and offered input and output compare identical
under the schema-comparison profile. Check all offered identifiers against keys
and aliases; more than one existing operation is a conflict. Do not rename an
offered operation or reshape its schemas to evade a collision.

For nontrivial reuse, use the optional
[comparison companion](../crates/openbindings-schema-comparison/README.md).
Comparison inability is neither equality nor difference. Report an omitted
new offering as `implementation-unsupported` and a corresponding existing entry
as `unassessed` when you cannot judge it. Do not add a duplicate binding for a
current, stale or unassessed one. Schema and identifier conflicts belong in
`omitted` as `conflict`, carrying `existing` and `offered` evidence.

Account for every existing binding naming the source, every schema the source
offers under an existing key, and the source itself when there is something to
report. `current` means the kind would write it unchanged; schema annotations
stripped by the comparison profile do not make it stale. `unmatched` means
nothing read corresponds to it; a partial read does not prove removal.
`unassessed` requires `reasonCode` and `message` and is no reason to delete it.
`stale` requires a proposal and a `breaking`, `dangerous` or `non-breaking`
change judgment. Use `dangerous` when classification is uncertain.

Source and binding data compare as JSON values. Existing-entry schema comparison
also needs report context: a reference to a target accounted for by the report
stays a leaf, so its target's drift is reported once at that target. Other
references are followed; a difference only through a target the report does not
account for makes the entry unassessed. Use the comparison companion's
`identical_with_references(left, right, accounted_for, limits)` with a callback
that identifies targets your report accounts for. Its `ReferenceContext` exposes
`spelling()`, the original `schema()`, `resource()` and `base()`, so accounting can
distinguish equal spellings in different resources. It preserves same-spelled
accounted references as leaves and keeps their siblings and containing allOf
unmerged. If it returns false, compare again with `&|_| true`: a true answer then
means the entry differs only behind unaccounted targets, so report `unassessed`.
A normalization failure also provides no identity answer. Use
`identical_with_references_and_control` when cooperative cancellation is needed.
The larger pull example demonstrates these calls; the minimal example has no
references.

Binding proposals can change binding `data` and the operation's `input` and
`output`, or remove those slots with a nonempty `remove` array. Source proposals
can replace `data` or remove it with `remove: ["data"]`. A field cannot be set
and removed in the same proposal. Omission leaves a field unchanged; `data:null`
sets JSON null. Schema proposals carry `{schema}`. A proposal cannot
overwrite author-owned descriptions, aliases, preferences, idempotence or other
annotations. Retained unknown metadata has no edit authority.

`omitted` distinguishes `excluded` under an identified kind rule, `invalid`
source content, `implementation-unsupported`, and `conflict`. An implementation
restriction is not a kind exclusion. `lossy` on an added entry is a nonempty
array of `{reasonCode, message, details?}` disclosures, one per fidelity loss.
Disclosure does not excuse an unsound binding. Report messages and details must
exclude secrets.

`report().is_complete()` means exhaustive, with no implementation-unsupported
omission and no unassessed existing entry. Conflicts, invalid or excluded items,
and disclosed losses do not make that condition false. The application can
require more. Completion is a producer claim, not proof of availability or
truthfulness.

### What goes in `uses`

On an added entry, `uses` lists the other additions the binding's data needs.
On a binding proposal, it lists the additions the proposed `data` needs. Use
`schemas/<key>`, `operations/<key>` or `bindings/<key>` labels, each naming an
addition, without duplicates or the binding itself. Only the kind can read those
needs inside binding data. List those needs and nothing else. Do not list what
a schema reference shows, and do not repeat what another addition needs in turn.

For example, if binding data names `ApiError`, list `schemas/ApiError`. If the
operation's output refers to `User`, and `User` refers to `Address`, leave both
out of `uses`. Their references already show the targets. A schema proposal is
`{schema}` and has no list of needs. An addition may exist for a proposal even
when no added binding needs it.

To keep part of a pull:

1. Choose the `added` entries to keep and insert their bindings.
2. Insert each binding's operation when it is an addition, and the additions
   its `uses` names.
3. Follow schema references from what you keep. Insert each target that is an
   addition. Follow references through existing entries too.
4. For every added binding reached along the way, insert its operation when
   added and what its own `uses` names. Repeat until nothing new comes in.
5. Validate the edited document before adopting it.

These steps are application code. `validate_for` returns the document holding
every addition, where references resolve, and its `references` report follows
`$id` and anchors.

The three recipes below form one module of the application, separate from the
adapter above, and need these imports:

<!-- guide:prelude -->
```rust
use openbindings::{
    ConformantDocument, Edit, JsonValue, ReferenceReport, ReferenceResolution, WorkControl,
};
use openbindings_source_pull::{
    Change, ExistingEntry, ExistingTarget, Proposal, PullLimits, PullRequest, PullResult,
};
use std::collections::{BTreeMap, BTreeSet};

/// What these application functions return on failure.
pub type Error = Box<dyn std::error::Error>;
```

<!-- guide:partial -->
```rust
/// Keep chosen added bindings and what they reach.
pub fn keep_part(
    result: &PullResult,
    request: &PullRequest,
    bindings: &[&str],
    limits: &PullLimits,
    control: &dyn WorkControl,
) -> Result<ConformantDocument, Error> {
    // References resolve in the document holding every addition.
    let everything = result.validate_for(request, limits, control)?;
    let ReferenceReport::Complete { references } = everything.document().references() else {
        return Err("reference inspection stopped early".into());
    };

    let additions = result.additions();
    let is_addition = |pointer: &str| additions.value().value().at(pointer).is_some();
    let mut wanted: Vec<String> = bindings
        .iter()
        .map(|key| format!("/bindings/{key}"))
        .collect();
    if wanted.iter().any(|pointer| !is_addition(pointer)) {
        return Err("a selected binding is not an addition".into());
    }

    let mut seen = BTreeSet::new();
    while let Some(pointer) = wanted.pop() {
        // A reference into an addition keeps the whole entry.
        let entry = pointer.split('/').take(3).collect::<Vec<_>>().join("/");
        let root = if is_addition(&entry) { entry } else { pointer };
        if !seen.insert(root.clone()) {
            continue;
        }
        // An added binding needs its added operation and what its `uses` names.
        if let Some(key) = root.strip_prefix("/bindings/")
            && let Some(added) = result.report().added().find(|added| added.binding() == key)
        {
            if !added.reused() {
                wanted.push(format!(
                    "/operations/{}",
                    added.operation().as_str().unwrap()
                ));
            }
            for label in added.uses().into_iter().flatten() {
                wanted.push(format!("/{}", label.as_str().unwrap()));
            }
        }
        // Everything this entry refers to, through additions and existing entries.
        let inside = format!("{root}/");
        for reference in &references {
            let from = reference.location.pointer.as_str().unwrap_or_default();
            if let (true, ReferenceResolution::Located { target, .. }) =
                (from.starts_with(&inside), &reference.resolution)
                && target.source_uri.is_none()
            {
                wanted.push(target.pointer.as_str().unwrap().to_owned());
            }
        }
    }

    // `Edit::set` needs its parent, and one batch cannot create a map and fill it.
    let mut document = request.interface().document().clone();
    for map in ["schemas", "bindings"] {
        let prefix = format!("/{map}/");
        let needed = seen
            .iter()
            .any(|pointer| pointer.starts_with(&prefix) && is_addition(pointer));
        if needed && document.value().get(map).is_none() {
            document = document.edit(&[Edit::set(&[map], JsonValue::parse("{}")?)])?;
        }
    }
    let edits: Vec<Edit> = seen
        .iter()
        .filter_map(|pointer| Some((pointer, additions.value().value().at(pointer)?)))
        .map(|(pointer, value)| Edit::set_at(pointer.as_str(), value.to_owned()))
        .collect();
    if edits.is_empty() {
        return Ok(request.interface().clone());
    }
    let edited = document.edit(&edits)?.validate()?;
    edited
        .conformant()
        .ok_or_else(|| "the kept document does not conform".into())
}
```

This walk sets no work limits of its own, which suits a source you trust.

When applying a proposal, copy its permitted fields and remove what `remove`
names. Retaining unknown proposal metadata gives it no edit authority.

<!-- guide:proposals -->
```rust
/// One proposal as document edits: what it carries, copied onto the existing
/// entry, and what its `remove` names. Unknown proposal metadata is not copied.
pub fn proposal_edits(request: &PullRequest, entry: &ExistingEntry) -> Vec<Edit> {
    proposal_writes(request, entry)
        .into_iter()
        .map(|(path, value)| {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            match value {
                Some(value) => Edit::set(&path, value),
                None => Edit::remove(&path),
            }
        })
        .collect()
}

fn proposal_writes(
    request: &PullRequest,
    entry: &ExistingEntry,
) -> Vec<(Vec<String>, Option<JsonValue>)> {
    let mut writes = Vec::new();
    let mut write = |path: &[&str], value| {
        writes.push((path.iter().map(|part| (*part).to_owned()).collect(), value));
    };
    match (entry.target(), entry.proposal()) {
        (ExistingTarget::Binding(key), Some(Proposal::Binding(proposal))) => {
            let key = key.as_str().unwrap();
            let binding = request.interface().binding(key).unwrap();
            let operation = binding.operation_key();
            if let Some(data) = proposal.data() {
                write(&["bindings", key, "data"], Some(data));
            }
            if let Some(input) = proposal.input() {
                write(&["operations", operation, "input"], Some(input));
            }
            if let Some(output) = proposal.output() {
                write(&["operations", operation, "output"], Some(output));
            }
            for field in proposal.remove().into_iter().flatten() {
                match field.as_str().unwrap() {
                    "data" => write(&["bindings", key, "data"], None),
                    field => write(&["operations", operation, field], None),
                }
            }
        }
        (ExistingTarget::Schema(key), Some(Proposal::Schema(proposal))) => {
            write(&["schemas", key.as_str().unwrap()], Some(proposal.schema()));
        }
        (ExistingTarget::Source(key), Some(Proposal::Source(proposal))) => {
            let key = key.as_str().unwrap();
            write(&["sources", key, "data"], proposal.data());
        }
        _ => {}
    }
    writes
}
```

After taking everything a pull offers, what a proposal needs is already in the
document, so there is nothing to follow:

<!-- guide:updates -->
```rust
/// Take everything the pull offers, then every non-breaking update. Every
/// addition is present, so what a proposal needs is already there.
/// Agreeing writes share one edit; contradictory writes require a caller decision.
pub fn take_non_breaking_updates(
    result: &PullResult,
    request: &PullRequest,
    limits: &PullLimits,
    control: &dyn WorkControl,
) -> Result<ConformantDocument, Error> {
    let everything = result.validate_for(request, limits, control)?;
    let mut writes: BTreeMap<Vec<String>, Option<JsonValue>> = BTreeMap::new();
    for (path, value) in result
        .report()
        .existing()
        .into_iter()
        .flatten()
        .filter(|entry| entry.change() == Some(Change::NonBreaking))
        .flat_map(|entry| proposal_writes(request, &entry))
    {
        if let Some(previous) = writes.get(&path) {
            let agrees = match (previous, &value) {
                (None, None) => true,
                (Some(previous), Some(value)) => previous.semantic_eq(value) == Some(true),
                _ => false,
            };
            if !agrees {
                return Err(
                    "non-breaking proposals write different values to the same slot".into(),
                );
            }
        } else {
            writes.insert(path, value);
        }
    }
    let edits: Vec<Edit> = writes
        .into_iter()
        .map(|(path, value)| {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            match value {
                Some(value) => Edit::set(&path, value),
                None => Edit::remove(&path),
            }
        })
        .collect();
    let edited = everything.document().edit(&edits)?.validate()?;
    edited
        .conformant()
        .ok_or_else(|| "the edited document does not conform".into())
}
```

Two proposals can write one place when their bindings share an operation.
The recipe coalesces equal writes and refuses contradictory writes so the caller
can choose which to apply. To apply a proposal while keeping only part of a pull,
apply it first and inspect references in that edited document, since a report of
the original cannot see inside a proposal. Then start the steps above from its
proposed schemas and, for a binding proposal, what its `uses` names.

Existing binding entries use `references` to name what their data refers to in
the input document. That list also leaves out what schema references show in
turn. Follow those references to understand which changes reach the binding.

### What the SDK checks

`PullResult::from_json` checks carrier structure and local field relationships.
`pull_source` then checks request/source existence, handoff and relationship
limits, new keys, source attribution, added coverage, existing binding coverage,
primary operation keys, reuse placement and uses labels. It also
builds a prospective document with every addition and validates core conformance.
It returns `(result, prospective)`, retaining that proof and the original JSON
limits. The caller chooses whether to adopt it. The original remains unchanged
and stale proposals are not applied.

It does not prove source exhaustiveness, faithful projection, schema identity
behind reuse, change judgments or needs hidden in binding data. It does not
reject every unused operation or schema addition; an addition can serve a
proposal without an added binding. Keep the report and additions honest. Core
conformance alone does not establish those kind-owned claims.

`PullResult::validate_for` performs the same validation for a supplied result
without dispatching a pull. It returns the prospective conformant document,
which the application may explicitly adopt. For partial adoption, insert only
the selected entries and their required additions. Apply proposal fields through
explicit document edits, then revalidate. Do not copy the entire proposal as a
binding or overwrite unrelated author fields.

## Run the application and tests

This test is a complete application flow. It chooses all additions, then invokes
the added binding. It uses Tokio only as a test executor; the adapter itself has
no runtime dependency.

<!-- guide:application -->
```rust
#[tokio::test]
async fn pull_then_invoke() -> Result<(), Box<dyn std::error::Error>> {
    use kind_adapter::Memory;
    use openbindings::Document;
    use openbindings_invocation::{
        BindingInvokerExt, BindingRequest, CancellationToken, InvocationError, OutputEvent,
    };
    use openbindings_source_pull::{PullRequest, SourcePullerExt};

    let original = Document::parse(
        r#"{
        "openbindings":"0.2.0", "operations":{},
        "sources":{"s":{"kind":"example.memory@1","data":{"channel":"local"}}}
    }"#,
    )?
    .validate()?
    .conformant()
    .ok_or("nonconformant document")?;
    let adapter = Memory;
    let request = PullRequest::new(original, "s");
    let (result, interface) = adapter
        .pull_source(request.clone(), CancellationToken::new())
        .await?;
    assert!(result.report().is_complete());
    // This application chooses every addition. No stale proposal is applied.
    let call = adapter.invoke(
        BindingRequest::new(interface, "echo.s"),
        CancellationToken::new(),
    )?;
    let (mut input, mut output) = call.split();
    let sending = async {
        input
            .send(openbindings::JsonValue::parse(
                "123456789012345678901234567890",
            )?)
            .await?;
        input.close();
        Ok::<(), Box<dyn std::error::Error>>(())
    };
    let receiving = async {
        let mut count = 0;
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(value) => {
                    assert_eq!(value.text(), "123456789012345678901234567890");
                    count += 1;
                }
                OutputEvent::Complete => {
                    assert_eq!(count, 1);
                    return Ok(());
                }
                OutputEvent::Error(error) => return Err(error),
                _ => (),
            }
        }
        Err(InvocationError::transport_closed())
    };
    let (sent, received) = tokio::join!(sending, receiving);
    received?;
    sent?;
    Ok(())
}
```

Run `python3 tools/verify-kind-adapter.py --wasm` from the SDK root. It checks
that every Rust fence matches compiled source, tests the external crate, runs
format and strict Clippy checks, verifies locked dependency identities, and
builds the Wasm library. It uses offline resolution and the example's own
lockfile. Native tests cover exact values, repeat pull, alias reuse, conflicts,
removal proposals, errors, preflight, cancellation and limits. They also stream
1,000 values past queue capacity and exercise a local future containing `Rc`:
cancellation drops its guard without resuming async cleanup. Compile checks
retain `Send` for the plain adapter's invocation and pull futures.

There is no public adapter conformance kit today. The evaluator test-support
crate tests schema evaluators, not kind adapters. The SDK's invocation lifecycle
helpers are private. Use checked calls in your own tests and the pinned interface
fixtures as shape/reference examples; passing carrier admission alone does not
prove behavior. Start with the [invocation tests](../crates/openbindings-invocation/tests),
[pull tests](../crates/openbindings-source-pull/tests) and
[description tests](../crates/openbindings-kind-description/tests).

For a real adapter, test streamed input and output beyond queue capacity,
backpressure, late errors, abrupt transport closure and dropping pending work.
Test challenges before output and effects, ordinary preflight failures,
wrong-kind and malformed data, absent versus null, wide numbers and safe errors.
Test report coverage, aliases, repeated pulls, removal and unassessed entries,
comparison inability, losses, opaque dependencies and author-field preservation.
Run both native and Wasm builds and exercise any actual host transport separately.

## Mistakes to check before publishing

- Do not wait for cancellation and then await cleanup in the dropped future.
- Do not compose exact values through ordinary floating-point JSON values.
- Use operation primary keys in reports, and `is_valid_name` for generated names.
- Keep `can_invoke`, support lists and descriptions consistent.
- Treat comparison inability as a gap, and a dropped schema as a removal proposal.
- Read fixed SDK categories with `Display`; inspect fields and accessors explicitly
  for values. Apply your disclosure policy before printing native cause chains.
- Retain invocation control when diagnosing rejected or oversized errors.
- Build for Wasm, then verify the host imports and local-future requirements.

For the full current contracts, read the pinned
[binding invocation](https://github.com/openbindings/interfaces/blob/a3c19658fc89815d7a417eae29ba31fd4e930daf/binding-invoker/README.md),
[kind description](https://github.com/openbindings/interfaces/blob/a3c19658fc89815d7a417eae29ba31fd4e930daf/kind-descriptor/README.md) and
[source pull](https://github.com/openbindings/interfaces/blob/a3c19658fc89815d7a417eae29ba31fd4e930daf/source-puller/README.md)
publications. Interface and SDK versions are independent.
