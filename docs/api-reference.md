# API reference and compatibility contracts

Definition-level Rustdoc and JSDoc cover the public SDK surface. Start with the
[Rust first-use guide](rust-first-use.md) or [TypeScript guide](../packages/typescript/README.md)
for a complete job; use the definitions for exact result, ownership and limit contracts.

| Surface | Definition source | Reference generation |
| --- | --- | --- |
| Rust document/authoring/contracts | [`openbindings`](../crates/openbindings/src/lib.rs), including its exact-value reexports | `cargo doc --locked --no-deps -p openbindings` |
| Rust default evaluator | [`openbindings-json-schema-evaluator`](../crates/openbindings-json-schema-evaluator/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-json-schema-evaluator` |
| Rust evaluator contract kit | [`openbindings-schema-evaluator-test-support`](../crates/openbindings-schema-evaluator-test-support/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-schema-evaluator-test-support` |
| Rust HTTP discovery | [`openbindings-http-discovery`](../crates/openbindings-http-discovery/src/lib.rs), including native adapter | `cargo doc --locked --no-deps -p openbindings-http-discovery --features native` |
| Rust source inspection role | [`openbindings-source-inspection`](../crates/openbindings-source-inspection/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-source-inspection` |
| Rust interface synthesis role | [`openbindings-interface-synthesis`](../crates/openbindings-interface-synthesis/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-interface-synthesis` |
| Rust binding and operation invocation roles | [`openbindings-invocation`](../crates/openbindings-invocation/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-invocation` |
| Shared Rust cancellation definitions | [`openbindings-internal-cancellation`](../crates/openbindings-internal-cancellation/src/lib.rs), reexported by all four async companions | `cargo doc --locked --no-deps -p openbindings-internal-cancellation` |
| TypeScript core and HTTP discovery | [`internal.ts`](../packages/typescript/src/internal.ts), as exported by the two entry points | Build the package, then run `node packages/typescript/scripts/check-api-reference.mjs target/api-reference` |
| TypeScript testing | [`testing.ts`](../packages/typescript/src/testing.ts), exported by `@openbindings/sdk/testing` | Included in the TypeScript reference renderer |
| Compiled Wasm asset import | [`wasm-module.d.ts`](../packages/typescript/src/wasm-module.d.ts) | Host import type documented alongside `initialize` |

The TypeScript renderer reads emitted declarations, follows reexports, and includes
inherited SDK members. It writes HTML plus a machine-readable inventory. The
`openbindings-internal-json` backend/numeric helpers and generated Wasm bindings
are implementation details, not additional supported SDK entry points. Exact JSON
contracts reexported by `openbindings` remain documented at their original definitions.
Rust's standard trait implementations inherit their standard contracts; the evaluator
and transport traits document their additional SDK obligations explicitly.

All public Rust types implement `Debug` as a structural aid: types, variants,
fixed SDK reasons and byte coordinates may be shown. Caller- and document-owned
text is omitted, including reference spellings, URLs, origins, JSON Pointers,
evidence-map keys and values, diagnostic messages, schema operands, bodies,
credentials, provider state and invocation context. `Display`, accessors and
serialization retain their existing output; use them when disclosure is intended.
Debug formatting is not a stable serialization format. Public authoring and evaluator records retain
struct-literal construction. Result-returning APIs document caller-visible failures
under `# Errors`; validation outcomes (including `DocumentValidation`) and configuration methods warn when ignored.

## Optional Rust role APIs

Source inspection uses `SourceInspector` and `Inspector`; interface synthesis uses
`InterfaceSynthesizer` and `Synthesizer`. Invocation uses `binding::BindingInvoker`
with `binding::Invoker` or `operation::Invoker`. These provisional roles have no
public TypeScript role API.

| Public item | Contract |
| --- | --- |
| `binding::BindingPreflight` | Optional provider trait extending `BindingInvoker`; its `Preflight<'a>` associated future and `preflight` method report advisory context requirements or no prediction. |
| `binding::Invoker::preflight_with_provider`, `operation::Invoker::preflight_with_provider` | Opt into the provider hook after checked admission; require `P: BindingPreflight`. |
| `binding::Invoker::preflight_with_provider_and_control`, `operation::Invoker::preflight_with_provider_and_control` | The same provider preflight with caller cancellation, isolated from sibling attempts. |
| `InspectionLimitation::additional_fields` | `BTreeMap<String, JsonValue>` retaining additional exact evidence; `code`, `message` and `details` cannot be shadowed. |

Implement `BindingPreflight` to provide advisory preflight. The facades' ordinary
`preflight` methods admit the request and make no prediction. Provider preflight
is `Send` when the provider is `Sync` and its associated future is `Send`; local
futures and `Rc` providers are also supported.

## Boundaries to preserve

- Parsing and authoring create exact snapshots. Normative validation evaluates OBI
  document rules; only its conformant branch creates a `ValidatedDocument` proof.
  Only that proof constructs normative value contracts and an operation invoker.
  Parsed snapshots retain fallible editor reads. Wasm owns the Rust proof; plain
  TypeScript reports and forged wrappers cannot create it.
- The semantic partitions `Evidence`, `Conformance`, `ValueOutcome`,
  `ValueContractPreparation` and `Direction` are closed, supporting exhaustive handling.
  Interpretation and no-verdict cause families are extensible. Rust matchers need a
  fallback for these causes. TypeScript exposes the known reason union for its
  version and is distributed with the matching Wasm engine; it does not add an
  arbitrary-string or unknown-outcome branch. Public report/problem records remain
  constructible by custom evaluator authors.
- Rust borrowed views do not create new owners. Rust clones and TypeScript `retain()`
  share immutable state. TypeScript `ParsedDocument.value()`, `OperationView.value()`,
  `ExactJson.get()` and `ExactJson.at()` create **new disposable owners** on each call.
  New binding/source/dependency/example views follow this same owner-acquisition method rule;
  their frozen metadata is plain data. A traversal cursor and each yielded owner
  have independent lifetimes. Scoped editable drafts own their converted exact
  leaves; caller-inserted exact handles stay caller-owned. Explicit draft disposal
  releases converted leaves; abandoning an undisposed draft does not revoke leaves
  that are still reachable. Individual leaf finalizers provide best-effort cleanup.
  Byte/text copies are ordinary host values. Dispose owners deterministically; release
  does not promise lower process RSS or reduced Wasm capacity.
- Every preparation compiles independently. Retain a ready `ValueContract` for
  repeated validation without recompilation; it survives context drop. Resource sets are immutable, explicit and scoped to each context; URI
  identity never authorizes I/O. Core and default schema evaluation do no acquisition.
  For a missing resource, Rust `ParsedDocument::references()` and TypeScript
  `ParsedDocument.references()` expose explicit reference spellings and keyword
  locations; apply an application disclosure policy before logging these source facts.
- Ready means prepared, not decidable. Every value may still return
  `NoVerdict`. Contracts expose no resource-completeness classification. Applications
  own replacement policy and handle refusal at preparation and validation.
- Static inspection uses `ReferenceRefusal` and `ReferenceRefusalReason`, including
  cancellation, instead of value-outcome `NoVerdict`. Original keyword and target
  locations are preserved. Both parsed and validated documents support inspection;
  it performs no acquisition and makes no value-decidability claim.
- Limits state their units and defaults in definitions. Cancellation is cooperative,
  not a deadline or preemptive interrupt. A same-thread `AbortSignal` cannot run while
  synchronous Wasm occupies that thread. HTTP discovery can interrupt asynchronous
  acquisition and separately enforces a whole-attempt deadline. It retains observed
  response metadata on later body failure and complete bounded bytes on a later stop.
- Findings retain original byte coordinates. JSON Pointer escaping and safe text
  presentation are separate operations. Byte columns require conversion for UTF-16
  editors. A report's independent rule evidence survives finding-count and aggregate
  pointer-byte caps; diagnostic truncation must remain visible.

Value diagnostics use the typed `Diagnostics::{Coarse, Upstream}` policy, default
Coarse. See the [evaluator guide](../crates/openbindings-json-schema-evaluator/README.md).
Custom Rust evaluators constructing `ValueOutcome::Fails` supply a value-free
`message` alongside `problems` and `problems_complete`. Empty incomplete diagnostics
are valid established failures.
Custom evaluators receive a `SchemaRequest` with the original document, supplied
resources, entry and entry location. `evaluation_program(control)` provides the
strict §7 projection; read-only `EvaluationProgram::entry_uri()` and `resources()`
keep its contents aligned with provenance, and `original_location` recovers
original coordinates. See [evaluator integration](schema-evaluators.md) for the custom evaluator contract.

## HTTP discovery host policy

Rust deadline and outcome items are defined in
[`client.rs`](../crates/openbindings-http-discovery/src/client.rs); constants in
[`lib.rs`](../crates/openbindings-http-discovery/src/lib.rs); native constructors in
[`native.rs`](../crates/openbindings-http-discovery/src/native.rs).

| Rust public item | Contract |
| --- | --- |
| `AttemptTimeout::{Default, After(Duration), Unbounded}` | Whole-attempt policy: thirty seconds, a positive finite duration, or explicitly no deadline. Zero duration is a configuration error. Connect, TLS, headers, redirects, body and validation share one deadline. |
| `AttemptTimeout::duration()` | Returns `Result<Option<Duration>, ConfigurationError>`; `None` denotes unbounded and zero returns `invalid-timeout`. |
| `ClientOptions::timeout` | Defaults to `AttemptTimeout::Default`; successful reads and redirects never reset it. `max_document_bytes: usize` defaults to 1 MiB; explicit zero and `usize::MAX` are refused. |
| `DiscoveryTimer::{now, sleep_until}` | Host monotonic time and a wake-capable absolute timer in the same epoch, expressed as `Duration`. Dropping the timer releases it; local futures are supported. |
| `discover(origin, &options, &control, &timer, transport)` | Races one host timer and caller `CancellationToken` while invoking a repeatable one-hop callback. The callback must dispatch the normalized URL, expose redirects, deliver decoded bytes or a typed refusal, and release acquisition on drop. Connection-time DNS, peer and proxy policy belongs to the host. |
| `DEFAULT_TIMEOUT_SECONDS` | Thirty-second SDK default. |
| `MAX_REDIRECTS` | Ten followed redirects on native and portable checked paths. Browser Fetch owns its hidden redirect limit. |
| `native::Client::new(ClientBuilder, ClientOptions)` | Installs SDK redirect policy last and builds the client, retaining application TLS, resolver, proxy, credential and decoding choices. Refuses downgrade and credential-replay hops before dispatch. Owns one Tokio attempt timer. |
| `native::Client::with_delegated_policy(Client, ClientOptions)` | Accepts an already-built client through explicit host policy delegation. The application owns per-hop admission, the ten-redirect limit and credential handling. SDK deadline and cancellation handling still apply. |
| `DiscoveryOutcome::Timeout` | SDK or identified transport deadline expired; distinct from caller `Cancelled`. Caller cancellation wins when both are ready at the same poll. Observed metadata and complete bounded body, if available, remain in the receipt. |
| `ErrorKind::RedirectPolicy` | Admission, downgrade, hop-limit, credential-replay or observable hidden/unfollowed redirect refusal; never document nonconformance or absence. |
| `ErrorKind::Decode` | Native only when the SDK refuses Content-Encoding left undecoded by the host. Native body-read errors yield `BodyError` with `Network` and upstream messages, including corrupt-coded and truncated bodies; identified deadlines yield `Timeout`. Portable transports may report their own decoding failures as `Decode`. |

Rust `ClientOptions` literals must supply `timeout` or use `..ClientOptions::default()`.
Native callers pass a builder to `Client::new`; built clients use the explicit
delegated constructor. Portable discovery requires an explicit host timer. Admission codes
are `origin-policy`, `invalid-timeout` and `http-client-build`. Exhaustive outcome
matches must handle `DiscoveryOutcome::Timeout`.

TypeScript items are exported by
[`http-discovery.ts`](../packages/typescript/src/http-discovery.ts) and defined in
[`internal.ts`](../packages/typescript/src/internal.ts).

| TypeScript public item | Contract |
| --- | --- |
| `DiscoveryOptions.timeoutMs` | Whole-attempt milliseconds, default 30,000; a positive finite number or `"unbounded"`. Zero is refused. The SDK owns its timer and AbortController, and checks elapsed monotonic time around synchronous validation. |
| `DiscoveryResult.status: "timeout"` | SDK or identified host deadline expired. Caller abort remains `"cancelled"` with reason `"aborted"`; caller cancellation wins a simultaneous observation. Observed metadata and complete bounded bytes remain available. |
| `DiscoveryResult.error.kind` | For `"transport-error"` and `"body-error"`, the produced kinds are `"network"`, `"redirect-policy"` and `"other"`. Timeouts have their own status and Fetch exposes no codec diagnosis. |

All paths normalize origins and require HTTPS except literal IPv4 127/8 and IPv6
`::1`. Native and portable checked paths follow the five discovery redirect
statuses with admitted Location values and refuse every downgrade before dispatch.
Safe native construction also refuses A -> B -> B: tower-http 0.6.11 restores
original request headers while reqwest 0.13.5 strips credentials only when the next
origin differs from the immediately preceding one. Applications requiring such
chains use `with_delegated_policy` and own credentials across all hops.

TypeScript Fetch uses `redirect: "follow"`, checks final URL admission and refuses
an observable HTTPS-to-HTTP downgrade. Browser mixed-content blocking in secure
contexts and private-network rules govern hidden hops; the SDK cannot observe them.
Opaque/opaqueredirect responses mean policy cannot be established. Timers remain
cooperative at blocking polls and synchronous validation boundaries.

Publication uses typed `AllowOrigin` choices: Rust `Any` (default), `Omit`, `Null`,
or `Exact(String)`; TypeScript `"any"` (default), `"omit"`, `"null"`, or `{ origin: string }`.
Exact origins must be canonical ASCII browser-serialized tuple origins. Noncanonical scheme,
host or default-port spellings and opaque schemes are refused. Routes match the
well-known path exactly, preserving reserved escapes. The SDK enables no reqwest
gzip, brotli, deflate or stream features; applications opt into decoding on their
own dependency and client, with decoded-byte limits still enforced. See the
[discovery guide](../crates/openbindings-http-discovery/README.md) for host obligations.

## Document validation

Rust `validate_document(input)` validates exact JSON bytes using default admission
limits of 64 MiB and 1,000,000 nodes, with no nesting cap.
`validate_document_with_limits(input, JsonLimits)` accepts caller-selected byte, node
and nesting limits, matching the explicit-limits shape of `JsonValue::parse_with_limits`.
Both return `Result<DocumentValidation, VersionRefusal>`; only a conformant
validation provides a `ValidatedDocument` proof.

Admission precedes version selection. Until the complete JSON text is admitted,
no version declaration has been established. An admission refusal leaves every
rule inconclusive, even if an initial fragment appears to name another specification
line. After admission, version-refusal and OBI-01 priority are unchanged. Document
validation checks admitted schemas locally at any depth and carries escaped lone
surrogates exactly; native namespace and primary-reference comparisons use UTF-16
units. A violation established by any rule makes the document non-conformant.

### Finding-code catalog

These are stable `Finding.code` values for document validation. Messages are human
explanations, not machine identifiers. Multiple rules can share a code. Parse
locations use original byte coordinates; pointers are absent where a member name
cannot be represented in Unicode scalar text. Input admission refusals are not
violations. `InputError` outside whole-document validation retains its existing
code-based Display representation.

| Code | Rule | Meaning |
| --- | --- | --- |
| `input-limit` | OBI-01 through OBI-13 | Byte, node or caller-selected depth admission limit prevented validation. |
| `bom` | OBI-01 | A leading UTF-8 byte-order mark is forbidden. |
| `utf8` | OBI-01 | The input is not valid UTF-8. |
| `comma-or-close` | OBI-01 | A container needed a comma or closing delimiter. |
| `object-key` | OBI-01 | A quoted object member name was expected. |
| `colon` | OBI-01 | A colon was expected after the member name. |
| `trailing-token` | OBI-01 | Content follows the complete JSON value. |
| `number` | OBI-01 | A JSON number is malformed. |
| `fraction` | OBI-01 | A decimal point lacks following digits. |
| `exponent` | OBI-01 | A number exponent lacks digits. |
| `value` | OBI-01 | A JSON value was expected. |
| `literal` | OBI-01 | A true, false or null literal is malformed. |
| `unterminated-string` | OBI-01 | A string lacks its closing quote. |
| `string-control` | OBI-01 | A string contains an unescaped control character. |
| `escape` | OBI-01 | A JSON string escape is invalid. |
| `unicode-escape` | OBI-01 | A Unicode escape lacks four hexadecimal digits. |
| `duplicate-member` | OBI-01 | An object repeats a decoded member name. |
| `schema-mismatch` | OBI-02, OBI-10 | A constraint in the derived schema or meta-schemas failed. The fixed message is `value violates the fixed normative schema`; locations identify deepest checked document positions or the checked schema node. |
| `fixed-schema-limit-or-failure` | OBI-02, OBI-10 | Fixed-schema checking could not complete within its capability or work limit. |
| `semver` | OBI-03 | The openbindings value is not a SemVer string. |
| `missing-version` | OBI-03 | The openbindings member is absent. |
| `name-grammar` | OBI-04 | A defined map key or operation alias violates the ASCII name grammar. |
| `duplicate-operation-name` | OBI-05 | An operation key or alias repeats another occurrence in that namespace. |
| `unknown-operation` | OBI-06, OBI-08 | A binding or dependency reference names no primary operation key. |
| `unknown-source` | OBI-07 | A binding reference names no primary source key. |
| `schema-dialect` | OBI-09 | A contained schema names a dialect other than JSON Schema 2020-12. |
| `absolute-schema-id` | OBI-11 | A document-resource schema ID is not an absolute URI. |
| `reference-type` | OBI-11 | A schema reference cannot be interpreted as a URI-reference string. |
| `reference-form` | OBI-11 | A document-resource reference is malformed or neither absolute nor same-document. |
| `reference-target` | OBI-12 | A same-document reference identifies no schema at an OBI position. |
| `duplicate-anchor` | OBI-13 | A plain name has multiple declarations in the document resource. |
| `duplicate-schema-id` | OBI-13 | Multiple contained schemas declare the same compared identifier. |

Raw parser admission codes `byte-limit`, `node-limit` and `depth-limit` remain
`InputError` codes; whole-document findings translate them to `input-limit`.

## Validated document reads

`ValidatedDocument::value_contracts` returns `ValueContracts` directly;
`operation::Invoker::new` takes `ValidatedDocument` and `with_evaluator` returns
`Self`. Preparation still may refuse, and Ready means prepared, not decidable.
`SchemaRequest::document()` exposes the validated snapshot to custom evaluators.

| Rust read | Result and presence |
| --- | --- |
| `openbindings()` | `&str`, proved supported SemVer |
| `name()`, `version()`, `description()` | `Option<JsonStringRef>`, absence separate from non-scalar text |
| `schemas()`, `schema(key)` | Optional exact object/boolean schema views; no preparation |
| `operations()`, `resolve_operation(name)` | Retained `ValidatedOperationView` vector or optional selection |
| `bindings()`, `sources()`, `dependencies()` and keyed reads | Optional retained `ValidatedBindingView`, `ValidatedSourceView`, `ValidatedDependencyView`; absent differs from present-empty |
| Validated operation `examples()` and `example(key)` | Retained `ValidatedExampleView`; example truth is not implied |

Validated views expose the same normative fields as parsed views, with infallible
reads. Required names are scalar; descriptive text, tags and kinds use
`JsonStringRef`. Its `as_str()` returns None only for non-scalar text;
`utf16_units()` iterates without allocating, `json()` retains token/location access,
and equality compares decoded UTF-16. `JsonRef::as_string()` returns None only for
non-strings. The existing fallible view names remain unchanged; the `Validated`
prefix identifies the additional proof, following `ValidatedDocument`.

TypeScript keeps its existing metadata and selection shapes. Validated documents
inherit exact editor reads and add `valueContracts()` and `parsed()`. Top-level
text is exact JavaScript UTF-16; optional text uses undefined for absence.
Each top-level text getter checks only its own field, preserving valid siblings
and absence when another metadata field is malformed.
`schemas()` creates a new ExactJson owner for each map value on every call.
`OperationView.input()` and `.output()` each create a new independent ExactJson owner,
preserving undefined separately from null, false, true and objects. Dispose each
acquisition; these methods make acquisition explicit. Plain text, metadata and editable draft
`value` properties remain getters. `initialize()` reads the installed Wasm asset
on Node without fetch; browsers retain module-relative loading. Explicit bytes
or a compiled module bypass default loading in either host. Cleanup assertions
import `liveStorageOwners` from `@openbindings/sdk/testing`; production entries
export no test bookkeeping.

## Inspection and exact editing

Rust `ParsedDocument` enumerates and looks up retained binding, source and
dependency views. `OperationView` retains its existing binding-key association,
adds tags/deprecation, and exposes named example views. Borrowed fields are tied
to their view; clone a view or retain its `JsonRef` with `to_owned()` when needed.
Namespace enumeration distinguishes absent from empty, and field accessors reject
malformed metadata with original locations. It does not assert conformance or
resolve source availability. TypeScript projects whole namespace metadata eagerly
and checks every row; a keyed view checks only its selected metadata when read.
Existing global interpretation prerequisites still apply.

TypeScript exact member/element traversal uses one lazy disposable iterator
convention. `ParsedDocument.toDraft()` creates an `OwnedDocumentDraft` whose
`EditableDocumentDraft` value provides writable normative maps/lists and exact
opaque leaves. The existing readonly-friendly `DocumentDraft` input and
`authorDocument` function remain. Conversion, editing, building and normative
validation are separate; no operation-reference rewriting or implicit acquisition
is introduced. See the shipped [inspection/edit caller](../packages/typescript/examples/inspect-edit.ts).

`AuthoringFailure.sourceLocation`, when present, addresses original source bytes;
`draftPointer` addresses the caller's draft. The two locations serve different
purposes. Parsed descriptions, tags and kinds use exact `JsonStringRef` in Rust.

Document findings deduplicate explanations for the same source occurrence
before presentation caps. Eliminating a repeat does not mark truncation; omitting
a distinct finding does. Equal schema text at different original locations still
produces distinct diagnostics. Repeated member names point to their offending
key tokens, including distinct byte offsets at an otherwise identical pointer.
Per-rule evidence remains independent of finding presentation. Value-problem
diagnostics use the separate typed policy described below.

## Reference verification

See the [maintainer guide](maintainers.md) for rendering and verification commands.

## Value diagnostics

`Diagnostics::Coarse`
(the default) returns `Fails` with the fixed message “value does not satisfy the
contract”, no problems, and `problems_complete = false`. TypeScript uses
`diagnostics: "coarse"`, `message`, and `problemsComplete: false`.

Select `DefaultEvaluator::new().with_diagnostics(Diagnostics::Upstream)` or
`proof.valueContracts({ diagnostics: "upstream" })` to request the library's own
first error through `validate()`. Messages remain value-free; available paths map
to original locations and optional schema facts may expose source operands.
This mode is **library-bounded**: construction can allocate in proportion to inputs
and can overflow the stack on deep values. The SDK promises nothing stronger than
the library. One first error does not establish diagnostic completeness, so the
flag remains false. Work/regex limits and cancellation can omit detail after a
verdict; missing detail never weakens failure and never counts as rule evidence.

## Caller-owned operation preparation

`operation::Invoker::prepare(name)` and `prepare_with_control(name, &control)`
return `Result<PreparedOperation<'_, P>, InvocationError>`. The handle borrows the
invoker, eagerly prepares both declared directions and owns their compiled state.
Absent directions need no evaluator. Failed or cancelled preparation releases all
partial work and returns no handle. Preparation does not select bindings or call
providers; ready means prepared, not that every value is decidable.

`PreparedOperation::invoke(attempt)` and `invoke_with_control(attempt, &control)`
take `operation::Attempt`, which carries only what varies per call: binding choice
and optional opaque context. `Attempt::new()` (also `Default`) requires a sole
supported binding; `Attempt::ordered(&[..])` chooses the first supported binding
in the supplied list; `Attempt::binding(key)` selects one exact binding. Every
ordered key must belong to the prepared operation, including keys after the first
supported entry. An exact binding belonging to another operation returns
`ERR_OPERATION_NOT_FOUND` before provider work. `.with_context(&context)` forwards
an opaque object; Debug redacts binding choices and context contents. Binding
support and selection are checked anew, and cancellation, queues and validation
evidence are independent. Plain `Invoker::invoke` keeps `Request` and still
prepares lazily once per used direction per attempt.

`ValidationReport` retains release, revision, conclusion, evidence, findings and
`findingsTruncated`; it has no application policy identifier. Applications attach
policy provenance to their own records.

## Cancellation boundary

Core and evaluator APIs accept `&dyn WorkControl`. The trait requires
`is_cancelled()` and defaults `check()` to `Result<(), Cancelled>`; `AtomicBool`
implements acquire observation. Core convenience APIs use a private always-live
control. Cancellation is permanent during an attempt; reset during work is unsupported.

Discovery, invocation, source inspection and interface synthesis reexport one
`CancellationToken` and `Cancellation` identity. Their controlled APIs borrow the
token, and providers receive an owned token. Token `check()` returns `Cancelled`,
not a value verdict. Contract boundaries convert it to `caller-cancelled`; async
consumers retain their existing cancellation outcomes. Invocation retains its
synchronous relay and isolated local tokens. See the [cancellation definitions](../crates/openbindings-internal-cancellation/src/lib.rs).


Document findings are independent of the value diagnostics mode. Locations identify
original document positions or checked schema nodes. Missing detail and truncated
findings do not change validity or rule evidence. Use rule, code and evidence for
logic, not message text or finding count.
