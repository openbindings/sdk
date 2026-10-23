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
| Rust source inspection interface | [`openbindings-source-inspection`](../crates/openbindings-source-inspection/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-source-inspection` |
| Rust interface synthesis interface | [`openbindings-interface-synthesis`](../crates/openbindings-interface-synthesis/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-interface-synthesis` |
| Rust binding and operation invocation interfaces | [`openbindings-invocation`](../crates/openbindings-invocation/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-invocation` |
| Shared Rust cancellation definitions | [`openbindings-internal-cancellation`](../crates/openbindings-internal-cancellation/src/lib.rs), reexported by all four async companions | `cargo doc --locked --no-deps -p openbindings-internal-cancellation` |
| Shared Rust interface records | [`openbindings-internal-interfaces`](../crates/openbindings-internal-interfaces/src/lib.rs): `SupportedKind` and `KindSupportVerdict`, reexported by inspection, synthesis and invocation; `BindingValue` and `BoundaryError`, reexported by inspection and invocation | `cargo doc --locked --no-deps -p openbindings-internal-interfaces` |
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
credentials, provider state, invocation context, and codes outside the SDK's
fixed vocabulary, such as an application's `InvocationError` code or a custom
evaluator's. `Display`, accessors and serialization retain their existing output,
so `InvocationError` Display prints every code; use them when disclosure is
intended.
Debug formatting is not a stable serialization format.
`Document` owns private exact storage; construct and edit it through its methods. Result-returning APIs document caller-visible failures
under `# Errors`. `DocumentValidation`, `VersionDecision`, `ValueOutcome` and
`DiscoveryOutcome` are `#[must_use]`, and so is every method that returns a
modified value, such as `UndefinedResult::with_location` and the invocation
builders' `with_context`, `with_evaluator` and `with_validation_detail`.

## Records that may gain fields

Output records, the structs the SDK builds and hands to callers, are
`#[non_exhaustive]`: reports, findings, source and schema locations, value
problems, refusals, reference reports, kit receipts, discovery and publication
results, companion results and errors. Their fields stay public to read and
assign, and later versions may add fields without a breaking release. Outside the
SDK they cannot be built with a struct literal, and a destructuring pattern needs
`..`. The exception is `JsonMember`, which `JsonRef::members` yields and only
the SDK builds. It stays exhaustive: a JSON member is exactly a name and a value,
so it has no field to gain, and `let JsonMember { name, value } = member;` stays
valid. Records an evaluator, provider or transport must build have a constructor
for their required fields and `with_*` methods for optional ones:

| Builder | Records |
| --- | --- |
| Custom evaluators | `NoVerdict::new(reason, code, message).with_location(..)`, `UndefinedResult::new(code, message).with_location(..)`, `ValueProblem::new(instance, schema, code, message).with_details(..)`, `SchemaLocation::new(resource, pointer)` |
| Source inspectors | `SourceInspection::complete(targets)` or `partial(targets, limitation)` and `with_limitation(..)`, `BindableTarget::new(source_ref, binding).with_operation_key(..).with_operation(..)`, `InspectionLimitation::new(code, message).with_details(..).with_additional_fields(..)`, `SupportedKind::new(kind).with_description(..)` |
| Interface synthesizers | `SynthesisCandidate::new(document, coverage)`, `CoverageClaim::complete(entries)` or `partial(entries, limitation)`, `CoverageEntry::new(..)` then its assignable evidence fields, `CoverageLimitation::new(code, message)` with the same builders, `SupportedKind::new(kind)` |
| Binding invokers and transports | `binding::SupportedKind::new(kind).with_description(..)`, HTTP discovery `RequestError::new(kind, message)` |
| Version checks | `VersionRefusal::new(declared)`, `Some` only for a well-formed version outside the supported line |

`SupportedKind` and `KindSupportVerdict` are one type in every companion that
reexports them, so a provider serving inspection, synthesis and invocation can
return the same advisory list from each trait. A verdict answers for the job of
the checked caller that returned it. Likewise `BindingValue` is both a
`BindableTarget`'s binding and the binding a `BindingRequest` carries: invoke an
inspected target's binding as it is, or assign the author's keys with
`BindingValue::bind` to get a core `Binding`.

Input records, which callers build and the SDK reads, keep struct-literal
construction: typed authoring parts and `BindingValue`, `SchemaResource`,
`JsonLimits`, evaluator `Limits`, kit `Options`, discovery `ClientOptions`,
`PublicationOptions`, `HttpResponse`, synthesis requests, invocation carriers and
`InvocationLimits`. Add `..Default::default()` where a record implements
`Default`. Discovery's `Header`, a name and a value built on both sides, stays
exhaustive.

Closed partitions stay exhaustive, so a `match` can name every variant:
`Evidence`, `Conformance`, `ValueOutcome`, `VersionDecision`, `JsonKind`,
`OperationSelection`, `ReferenceResolution`, `CoverageScope`, `CoverageStatus`,
`operation::Direction` and `ValidationStage`. Adding a variant to one is a
breaking change, as is adding a field to a variant (an evaluator builds
`ValueOutcome::Fails { message, problems, problems_complete }` directly). Open
categories are `#[non_exhaustive]` and need a fallback arm: refusal and cause
reasons, error kinds, `InputErrorKind`, `Detail`, `DiscoveryOutcome`, discovery
`ErrorKind`, `PublicationError` and `WriteError`.

TypeScript has no equivalent annotation. Its result objects may likewise gain
properties in later versions: read the properties you use rather than relying on
an exact key set, and keep a default branch when switching on an open reason or
kind union.

## Rust document lifecycle

| Public item | Contract |
| --- | --- |
| `validate_document`, `validate_document_with_limits` | Judge bytes as they are, for reading: a JSON syntax or encoding error is an OBI-01 violation and an exceeded admission limit leaves every rule inconclusive, both in the report. Only an unsupported declared version is `Err`. |
| `Document::parse`, `Document::parse_with_limits(input, JsonLimits)`, `Document::from_json` | Admit exact JSON for editing, without a conformance claim, including documents with missing or mistyped fields. `parse` and `parse_with_limits` return `InputError` for invalid UTF-8, a byte order mark, a JSON syntax error or an exceeded admission limit; `parse` and `from_json` use `JsonLimits::default()`. |
| `validate_document_owned(input: Vec<u8>)`, `validate_document_owned_with_limits(input: Vec<u8>, JsonLimits)`, `Document::parse_owned(input: Vec<u8>)`, `Document::parse_owned_with_limits(input: Vec<u8>, JsonLimits)`, `JsonValue::parse_owned_with_limits(input: Vec<u8>, JsonLimits)` | Exactly the results, reports, errors, messages and locations of the borrowed forms, taking the bytes: the document or value keeps the caller's buffer as its text instead of a copy, so a caller that hands over its buffer holds one copy of the text, not two. |
| `Document::limits(&self) -> JsonLimits` | The admission limits the document was admitted under, which its edits and typed setters apply to the edited document: the caller's for `parse_with_limits` and `validate_document_with_limits`, no nesting cap for `validate_document`, the bytes of the configured body bound (at least the default) for a discovered document, and the defaults for `parse`, `new` and `from_json`. Value-contract preparation keeps fixed bounds of its own: a projection stops at 64 MiB (`program-byte-limit`) and each resource it writes fits 64 MiB, 1,000,000 nodes and 10,000 levels (`program-json-limit`). TypeScript's `limits` getter on a document reports the same. |
| `is_valid_name` | Tests the specification's ASCII name grammar without normalizing the name. |
| `Document::new` | Create an empty OBI declaring the authoring version. |
| `Document::set`, `remove`, `edit(&[Edit])` | Require a mutable borrow. One atomic splice and parse per batch, exact untouched bytes, compact insertions, the document's admission limits. A pointer is a `PointerRef`, converted from escaped text (`&str`, `&String`) or an exact `&Pointer`, so a finding's pointer through a member name with an unpaired UTF-16 unit addresses that member. |
| Typed field and part setters | Edit descriptions, names, versions, operation schemas and typed operations, bindings, sources, dependencies and examples. Map setters support bulk construction. |
| `DocumentValidation::document`, `conformant` | Borrow an admitted document or obtain proof only when every applicable rule is established. |
| `Conformant<Document>` | Private proof, infallible reads, sole route to value contracts. No unchecked constructor, deserialization, dereference or mutable borrow. |
| `as_document`, `into_inner` | Explicit plain reads, or consume proof to edit again. |
| `Document::as_bytes`, `EditError::code` | Exact current bytes and stable kebab-case editing failure codes. |

Source positions describe current bytes. Retained views, reports and proofs keep
their revision when another owner is edited. Duplicate members remain inspectable
for diagnostics and are refused by typed interpretation, editing and proof.

## TypeScript document lifecycle

| Public item | Contract |
| --- | --- |
| `Document.parse(bytes, { limits })` | Returns the existing parse-result union; admits exact JSON without a conformance claim, including documents with missing or mistyped fields, and returns `input-error` for an encoding or JSON syntax error or an exceeded admission limit. `limits` is a `Partial<JsonLimits>` (`maxBytes`, `maxNodes`, `maxDepth`), Rust's `JsonLimits`, whose unspecified entries keep its defaults; malformed limits or options throw `SdkError` with code `invalid-json-limits`. The document keeps the limits (its `limits` getter) and applies them to the edited document, as Rust does, on every edit route. Typed reads may throw. |
| `Document.create()` | Creates an empty OBI for construction through setters. |
| `set`, `remove`, `edit(edits)` | Mutable pointer edits; a batch crosses the bridge once, including exact inputs, and preserves untouched bytes. Exact inputs are borrowed. A pointer may hold an unpaired UTF-16 unit, as a finding's can, and addresses that member. Failure leaves bytes and caches unchanged. |
| Typed field and whole-part setters | Accept typed part values; map setters commit bulk construction once. |
| `Document.validate()` | Returns a `DocumentValidation`, the name Rust gives the same result: its `conformant` branch owns an independent `ConformantDocument` and a plain report; its other branches are a `DocumentReport`. |
| `validateDocument(bytes, { limits })` | Assesses exact source directly, including text that is not JSON, and returns a `DocumentReport`: plain data with a `report` or `version-refused` branch and no handle to dispose. Without `limits` it admits as Rust's `validate_document` does; with them, as `validate_document_with_limits` does. |
| `parseJson(bytes, { limits })` | Rust's `JsonValue::parse_with_limits`: an `ExactJson` or `input-error`. A value past the default limits is checked by parsing it here with larger limits and passing the `ExactJson` to `validate` or `explain`. |
| `ConformantDocument` | Nominal, privately constructed proof with infallible typed reads. `resolveOperation(name)` returns a `ConformantOperationView` or `undefined`. It has no setters and does not inherit `Document`. |
| `ConformantOperationView` | `input()` and `output()` return a disposable `ValueContract` or `undefined`; the plain `OperationView` of a `Document` returns exact values instead. Both views have `retain()`. |
| `ValueContract` | `schema()` acquires the exact schema; `prepare(options)` takes `resources`, `limits` and `signal` and returns `{ status: "ready", valueContract }` or `{ status: "no-verdict", detail }`. |
| `PreparedValueContract` | Disposable prepared value contract with `validate(value, options)`, the cheap check, `explain(value, options)`, which adds problems to a failure, and `retain()`; independent of its document, operation, resources and contract owners. `validateJson(input, options)` is Rust's `validate_json`: it admits JSON text or bytes as `parseJson` does and returns a `JsonCheck`, whose `input-error` carries the `parseJson` error. `validate` reads a string as a JSON string value. |
| `ExactJson.locate(pointer)`, `ConformantDocument.locate(at)`, `SchemaResources.locate(at)` | The `SourceLocation` of the node a problem's instance pointer or a `SchemaLocation` names, in the value, the document or the supplied resource with its URI, or `null`. A malformed pointer throws `SdkError` with code `invalid-pointer`, and a malformed schema location with code `invalid-schema-location`. |
| `asDocument()`, `intoDocument()` | Acquire an independent editable owner, or consume the proof. Consumption revokes all JavaScript aliases of that proof; retained owners survive. |
| `toBytes()`, `dispose()`, `Symbol.dispose`, `disposed` | Detached exact current bytes and the existing deterministic lifecycle. |

A `parseJson` value used in an edit preserves decimal spelling. Views retain their
revision independently, so editing an owner does not change an acquired view or
proof. `EditError.code` reports stable editing refusals; typed-part input errors
also expose `partPointer` without inventing source coordinates.

For construction, Rust's `Edit::SetOperations`, `SetDependencies`, `SetSources`
and `SetBindings` variants each borrow a typed map through their `value` field.
They can share an `edit(&[Edit])` batch with `Set` and `Remove`. This extends the
existing enum and whole-map setters without a separate builder or unrestricted
serialization API. TypeScript mirrors them as `set-operations`, `set-dependencies`,
`set-sources` and `set-bindings` discriminants with a `value` map. One batch crosses
the bridge once and commits one revision. Known byte extents are preflighted;
typed parts serialize once into a bounded writer that reserves all remaining
known bytes and stops before exceeding the budget. A refusal discards that buffer.

## Rust value contracts

| Public item | Contract |
| --- | --- |
| `ConformantOperationView::input(&self)`, `output(&self) -> Option<ValueContract<'_>>` | The operation's declared value contract, or `None` when the field is absent. Reading prepares nothing. Only a conformant view hands one out; a plain `OperationView` returns the exact `JsonRef`. |
| `ValueContract::schema(&self) -> JsonRef<'a>` | The exact unprepared schema; present `true`, `false` and objects remain distinct. |
| `ValueContract::prepare(&self, &dyn SchemaEvaluator) -> Result<PreparedValueContract, NoVerdict>` | Prepares with no supplied resources. A `&DefaultEvaluator` coerces; the evaluator is borrowed for the call. |
| `ValueContract::prepare_with(&self, &dyn SchemaEvaluator, &ResourceSet)`, `prepare_with_control(.., &dyn WorkControl)` | The same with immutable supplied resources and cooperative cancellation. A refusal judges no instance and does not prove undefinedness. |
| `PreparedValueContract::validate_json(input: impl AsRef<[u8]>) -> Result<ValueOutcome, InputError>` | Admits through `JsonValue::parse` with default limits, then validates. `Err` is admission failure only; all four value outcomes are `Ok`. Duplicate members yield no-verdict and numbers retain exact semantics. An exceeded limit's message names the route that admits more: `JsonValue::parse_with_limits`, then `validate` (TypeScript: `parseJson` with `limits`, then `validate`). |
| `PreparedValueContract::validate(&self, &JsonValue)`, `validate_with_control(.., &dyn WorkControl) -> ValueOutcome` | The cheap check: a failure carries no problems and `problems_complete` is false. |
| `PreparedValueContract::explain(&self, &JsonValue)`, `explain_with_control(.., &dyn WorkControl) -> ValueOutcome` | The same verdict, with problems that explain a failure. Library-bounded; see [value diagnostics](#value-diagnostics). |
| `JsonValue::locate(&self, &Pointer)`, `Conformant<Document>::locate(&self, &SchemaLocation)`, `ResourceSet::locate(&self, &SchemaLocation) -> Option<SourceLocation>` | Byte offset, line and column of the node a problem's instance pointer or schema location names, in the value's, document's or supplied resource's text; `None` for another source or a pointer naming no node. One pass on the call; nothing retained. |

`PreparedValueContract` is `Clone + Send + Sync + 'static` and owns what it needs after
the document, resources and evaluator are dropped. Core has no default evaluator
dependency. Use `JsonValue::parse_with_limits` and `PreparedValueContract::validate`
for custom admission limits or retained exact values (TypeScript: `parseJson` with
`limits`, then `validate`); `validate_with_control`
accepts cooperative cancellation. Each preparation still compiles independently.
A `ResourceSet` indexes its resource identities once, and clones share it. A
resolution space is assembled only when an evaluator requests the core projection,
once per live revision and set; repeated or alternating preparation reuses it. The
space without supplied resources stays with the revision, and a space with a set
stays with that set until the set or the revision is dropped, so a prepared
contract never retains a space for resources it did not receive.

A preparation refusal is a `NoVerdict`, with its Debug redaction: the broad
reason is shown, while its code, message and original schema text and
locations are redacted. Display and the public fields can expose
custom evaluator messages; callers choose their disclosure policy.

## Optional Rust companion APIs

Source inspection uses `SourceInspector` and `Inspector`; interface synthesis uses
`InterfaceSynthesizer` and `Synthesizer`. Invocation uses `binding::BindingInvoker`
with `binding::Invoker` or `operation::Invoker`. These provisional companion APIs
are not exported by the TypeScript facade.

| Public item | Contract |
| --- | --- |
| `INTERFACE_VERSION`, `APPLIED_INTERFACE_REVISION` | In invocation, source inspection and interface synthesis: the published interface version implemented, independent of the crate's own version, and the revision of the interfaces repository applied. HTTP discovery keeps its separate companion constants. |
| `CoverageClaim::complete(entries)` | Claims an exhaustive inventory with no limitation; the synthesis facade checks correspondence before returning `SynthesisCoverage`. |
| `ProviderInput: futures_core::Stream<Item = JsonValue>` | Poll-based input delivery sharing the inherent `next` implementation; preserves backpressure, cancellation, validation failure suspension and terminal ordering. `close` discards unread input. |
| `binding::BindingInvoker::preflight` | Provided method declaring the provider's advisory preflight hook; `None` by default. A hook returns a `Self::Invocation` job that completes with `Ok(())` for no prediction or with the `CONTEXT_REQUIRED` challenge invocation would raise. |
| `binding::Invoker::preflight`, `operation::Invoker::preflight` | Run the SDK's admission checks (and, for operations, selection) for every provider, then the provider's hook when it declares one. |
| `binding::Invoker::preflight_with_control`, `operation::Invoker::preflight_with_control` | The same provider preflight with caller cancellation, isolated from sibling attempts. |
| `InspectionLimitation::additional_fields` | `BTreeMap<String, JsonValue>` retaining additional exact evidence; `code`, `message` and `details` cannot be shadowed. |

Override `BindingInvoker::preflight` to provide advisory preflight. The
facades' `preflight` methods always admit the request and, when the provider
declares a hook, return its answer; a provider without one makes no prediction,
and invocation never requires preflight. The hook's job is the provider's
`Invocation` future, so stable Rust callers can bound it: binding preflight is
`Send` when the provider is `Sync` and `P::Invocation<'a>` is `Send`, and so are
operation preflight and plain operation invocation. A future that holds
a prepared operation also needs a `Send` provider, because the handle shares its
invoker.
Local futures and `Rc` providers are also supported.

## Boundaries to preserve

- Parsing and authoring create exact snapshots. Normative validation evaluates OBI
  document rules; only its conformant branch creates a `Conformant<Document>` proof.
  Only that proof constructs normative value contracts and an operation invoker.
  Documents retain fallible editor reads. Wasm owns the Rust proof; plain
  TypeScript reports and forged wrappers cannot create it.
- The semantic partitions `Evidence`, `Conformance` and `ValueOutcome` are closed,
  supporting exhaustive handling.
  Interpretation and no-verdict cause families are extensible. Rust matchers need a
  fallback for these causes. TypeScript exposes the known reason union for its
  version and is distributed with the matching Wasm engine; it does not add an
  arbitrary-string or unknown-outcome branch. Custom evaluator authors build
  problems and refusals with the constructors in
  [records that may gain fields](#records-that-may-gain-fields).
- Rust borrowed views do not create new owners. Rust clones and TypeScript `retain()`
  share immutable state. TypeScript `Document.value()`, `OperationView.value()`,
  `ExactJson.get()` and `ExactJson.at()` create **new disposable owners** on each call.
  New binding/source/dependency/example views follow this same owner-acquisition method rule;
  their frozen metadata is plain data. A traversal cursor and each yielded owner
  have independent lifetimes. See the TypeScript guide for editing ownership.
- Every preparation compiles independently. Retain a `PreparedValueContract` for
  repeated validation without recompilation; it survives document, resource and
  evaluator drop. Resource sets are immutable, explicit and scoped to the preparations that receive them; URI
  identity never authorizes I/O. Core and default schema evaluation do no acquisition.
  For a missing resource, Rust `Document::references()` and TypeScript
  `Document.references()` expose explicit reference spellings and keyword
  locations; apply an application disclosure policy before logging these source facts.
- `ValueOutcome::Undefined { detail }` establishes an undefined result under the
  value-contract rules. TypeScript uses `{ outcome: "undefined", detail }`.
  The public `UndefinedResult` record has `code`, `message` and `location` fields;
  Rust provides `new(code, message)` and `with_location(location)`. A missing
  location is `None` in Rust and `null` in TypeScript. Debug shows the recognized
  fixed code and redacts arbitrary codes, text and pointers. Explicit fields and
  serialization retain them. `NoVerdictReason` contains only inability causes.
- Prepared does not mean decidable. Every value may still return
  `NoVerdict`. Contracts expose no resource-completeness classification. Applications
  own replacement policy and handle refusal at preparation and validation.
- Static inspection uses `ReferenceRefusal` and `ReferenceRefusalReason`, including
  cancellation, instead of value-outcome `NoVerdict`. Original keyword and target
  locations are preserved. Both unproven and conformant documents support inspection;
  it performs no acquisition and makes no value-decidability claim.
- Limits state their units and defaults in definitions. Cancellation is cooperative,
  not a deadline or preemptive interrupt. A same-thread `AbortSignal` cannot run while
  synchronous Wasm occupies that thread. HTTP discovery can interrupt asynchronous
  acquisition and enforces a whole-attempt deadline when the application sets one.
  It retains observed response metadata on later body failure and complete bounded
  bytes on a later stop.
- Findings and value problems retain original byte coordinates with exact
  pointers. JSON Pointer escaping and safe text presentation are separate
  operations. Byte columns require conversion for UTF-16 editors. A report's
  independent rule evidence survives finding-count and aggregate pointer-byte caps;
  diagnostic truncation must remain visible.

Value diagnostics are chosen per call: `validate` is the cheap check and `explain`
asks for problems. See the [evaluator guide](../crates/openbindings-json-schema-evaluator/README.md).
Custom Rust evaluators receive the call's `Detail`. An evaluator constructing
`ValueOutcome::Fails` supplies a value-free `message` alongside `problems` and
`problems_complete`. Empty incomplete diagnostics are valid established failures.
Custom evaluators receive a `SchemaRequest` with the original document, supplied
resources, entry and entry location; `location(node)` names a node of the document
or a supplied resource. They build problems with `ValueProblem::new(instance,
schema, code, message)` and `with_details(details)`, an instance pointer from the
validated value and `SchemaLocation::new(resource, pointer)`, and refusals with
`NoVerdict::new(reason, code, message)` and `with_location(location)`. `evaluation_program(control)` provides the
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
| `ClientOptions::timeout: Option<Duration>` | Whole-attempt deadline. `None`, the default, sets none: the attempt runs until it completes or the caller cancels it. A deadline only bounds time, so a host that needs one sets `Some(duration)`. Zero returns `invalid-timeout`. Connect, TLS, headers, redirects, body and validation share one deadline; successful reads and redirects never reset it. |
| `ClientOptions::max_document_bytes: usize` | Decoded-body bound protecting the process; defaults to 1 MiB. Explicit zero and `usize::MAX` are refused. A body within it is validated under admission limits whose byte limit is this bound when it exceeds the default 32 MiB, with the default 1,000,000 nodes and any nesting, and the found document keeps them; a body past the nodes is undetermined, and validating `DiscoveryResult::body` with `validate_document_with_limits` admits it. TypeScript's `maxDocumentBytes` does the same. |
| `DiscoveryTimer::{now, sleep_until}` | Host monotonic time and a wake-capable absolute timer in the same epoch, expressed as `Duration`, consulted only when a deadline is set. Dropping the timer releases it; local futures are supported. |
| `discover(origin, &options, &control, &timer, transport)` | Races one host timer and caller `CancellationToken` while invoking a repeatable one-hop callback. The callback must dispatch the normalized URL, expose redirects, deliver decoded bytes or a typed refusal, and release acquisition on drop. It is called again for each admitted redirect, including a cross-origin one, and scopes credentials to the URL it is given. Connection-time DNS, peer and proxy policy belongs to the host. `final_url` is the URL the SDK last dispatched. |
| `MAX_REDIRECTS` | Ten followed redirects on native and portable checked paths, and in TypeScript where the host exposes redirects. Browser Fetch owns its hidden redirect limit. |
| `native::Client::new(ClientBuilder, ClientOptions)` | Installs SDK redirect policy last and builds the client, retaining application TLS, resolver, proxy, credential and decoding choices. Refuses downgrade and credential-replay hops before dispatch. Creates Tokio timers (the attempt timer when a deadline is set, and the bounded drain of a small non-200 body), so it requires a Tokio runtime with timers enabled. Credential headers other than reqwest's four standard ones follow cross-origin hops. |
| `native::Client::with_delegated_policy(Client, ClientOptions)` | Accepts an already-built client through explicit host policy delegation. The application owns per-hop admission, the ten-redirect limit and credential handling. SDK deadline and cancellation handling still apply. |
| `DiscoveryOutcome::Timeout` | SDK or identified transport deadline expired; distinct from caller `Cancelled`. Caller cancellation wins when both are ready at the same poll. Observed metadata and complete bounded body, if available, remain in the receipt. |
| `ErrorKind::RedirectPolicy` | Admission, downgrade, hop-limit, credential-replay or observable hidden/unfollowed redirect refusal; never document nonconformance or absence. |
| `ErrorKind::Decode` | Native only when the SDK refuses Content-Encoding left undecoded by the host. Native body-read errors yield `BodyError` with `Network` and upstream messages, including corrupt-coded and truncated bodies; identified deadlines yield `Timeout`. Portable transports may report their own decoding failures as `Decode`. |

Rust `ClientOptions` literals must supply `timeout` or use `..ClientOptions::default()`.
Native callers pass a builder to `Client::new`; built clients use the explicit
delegated constructor. Portable discovery requires an explicit host timer. Admission codes
are `origin-policy`, `invalid-timeout` and `http-client-build`. `DiscoveryOutcome`
and transport `ErrorKind` may gain variants, so matches need a fallback arm;
transports report failures with `RequestError::new(kind, message)`.

TypeScript items are exported by
[`http-discovery.ts`](../packages/typescript/src/http-discovery.ts) and defined in
[`internal.ts`](../packages/typescript/src/internal.ts).

| TypeScript public item | Contract |
| --- | --- |
| `DiscoveryOptions.timeoutMs` | Whole-attempt deadline in milliseconds. Omitted, the attempt has no deadline and runs until it completes or the caller's `signal` aborts it. When set it must be a positive finite number; zero is refused with `invalid-timeout`. The SDK then owns the timer, and checks elapsed monotonic time around synchronous validation. |
| `DiscoveryResult.status: "timeout"` | SDK or identified host deadline expired. Caller abort remains `"cancelled"` with reason `"aborted"`; caller cancellation wins a simultaneous observation. Observed metadata and complete bounded bytes remain available. |
| `DiscoveryResult.error.kind` | For `"transport-error"` and `"body-error"`, the produced kinds are `"network"`, `"redirect-policy"` and `"other"`. Timeouts have their own status and Fetch exposes no codec diagnosis. |

All paths normalize origins and require HTTPS except literal IPv4 127/8 and IPv6
`::1`. Native and portable checked paths follow the five discovery redirect
statuses with admitted Location values and refuse every downgrade before dispatch.
Safe native construction also refuses A -> B -> B: tower-http 0.6.11 restores
original request headers while reqwest 0.13.5 strips credentials only when the next
origin differs from the immediately preceding one. Applications requiring such
chains use `with_delegated_policy` and own credentials across all hops.

TypeScript Fetch requests `redirect: "manual"`. Where the host exposes the redirect
response, as Node does, the SDK checks each hop as the portable path does: it refuses
every downgrade and refused destination before dispatch, follows at most ten
redirects, calls the Fetch callback once per hop with only Accept so the callback
scopes credentials to the URL it is given, and reports the URL it last dispatched as
`finalUrl`. The callback must scope any credentials it adds by that URL; the SDK
cannot strip headers the callback adds itself. An opaqueredirect shows the transport
hides hops, as browser fetch does; the SDK then repeats the request with
`redirect: "follow"`, remembers that for the transport so later discoveries follow
directly, checks final URL admission and refuses
an observable HTTPS-to-HTTP downgrade, and a visible 301, 302, 303, 307 or 308 with
Location was left unfollowed and is `"redirect-policy"`, as in Rust. Browser mixed-content blocking in secure
contexts and private-network rules govern those hidden hops; the SDK cannot observe them.
Opaque responses mean policy cannot be established. Timers remain
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
limits of 32 MiB and 1,000,000 nodes, with no nesting cap.
`validate_document_with_limits(input, JsonLimits)` accepts caller-selected byte, node
and nesting limits, matching the explicit-limits shape of `JsonValue::parse_with_limits`
and `Document::parse_with_limits`. TypeScript's `validateDocument(input, { limits })`
mirrors both: without `limits`, `validate_document`; with them,
`validate_document_with_limits`, unspecified entries keeping the `JsonLimits`
defaults, including the 10,000-level nesting cap. The validated document keeps
the limits it was admitted under. `validate_document_owned` and
`validate_document_owned_with_limits` judge a `Vec<u8>` the caller hands over
exactly as the borrowed forms judge its bytes, and a document they keep holds
that buffer as its text.
Both return `Result<DocumentValidation, VersionRefusal>`; only a conformant
validation provides a `Conformant<Document>` proof.

Admission precedes version selection. Until the complete JSON text is admitted,
no version declaration has been established. An admission refusal leaves every
rule inconclusive, even if an initial fragment appears to name another specification
line. After admission, version selection follows §8.1 before applying this line's
OBI-01 exclusions. An escape denoting an unpaired surrogate in any member name or
string value violates OBI-01, including opaque content and embedded schemas. Its
finding locates the original token; OBI-02 through OBI-13 are not applicable.
Escaped surrogate pairs and noncharacters remain permitted. Exact parsing still
retains unpaired units for draft inspection, caller values and supplied resources.
Document validation checks admitted schemas locally at any depth. A violation
established by any applicable rule makes the document non-conformant.

OBI-02 applies the derived schema to the whole document. It runs on the
unmodified jsonschema 0.58.6 library with its default pattern engine. Every
derived pattern is regular, so that engine matches it on a finite automaton in
time linear in the string, with the results of ECMAScript's `u`-mode RegExp; the
derived schema has no recursive reference, so a node meets each of its 80
positions at most once. Any document the admission limits admit, default or
caller-selected, therefore gets an OBI-02 verdict, and the check's work is linear
in those limits; it has no work budget of its own. Location guidance revalidates
only beneath failing positions, a bounded number of times for each level of the
derived schema's finite depth, and stops at its findings allowance. Linear work is not short work: the SemVer near miss,
the worst case per byte, took about 0.35 seconds at the default 32 MiB limit,
0.6 seconds at 60 MB and 0.7 seconds at 64 MiB in a release build on an Apple M1 Max. Callers bound that
time with the admission byte limit, `JsonLimits::max_bytes` (`limits.maxBytes` in
TypeScript).
OBI-10 checks each embedded schema object against the meta-schema, its subschemas
separately, with recursive meta-schema edges cut to a type check, so one node
meets each of the cut meta-schema's 129 positions at most once; its patterns are
regular too. Every schema the admission limits admit therefore gets a
meta-schema verdict. A check reads its schema's own members and only the type of
each subschema, so a node is read by at most two checks and their work together
stays linear in the admission limits. A failing schema's members are read once
more to locate its findings at the keywords whose values fail.

### Finding-code catalog

These are stable `Finding.code` values for document validation. Messages are human
explanations, not machine identifiers. Multiple rules can share a code. Parse
locations use original byte coordinates. Pointers are exact, including a member
name with an unpaired UTF-16 unit (Rust `Pointer::as_str` is then `None`), and are
absent only for a syntax error, which precedes any value. Input-limit findings have
no location. Input admission refusals are not violations. Outside whole-document
validation, an `InputError` keeps its code and byte offset, and its `Display`
writes what the parser expected or found and the byte where, such as
`expected a colon after the member name at byte 5`, with the same wording as a
syntax finding's message; for an exceeded limit it adds the setting that raises
it, as in `; raise JsonLimits::max_bytes (limits.maxBytes in TypeScript)`, or,
from `validate_json`, which takes no limits, parsing with
`JsonValue::parse_with_limits` and a larger limit, then validating, and from
`JsonValue::string` parsing the string's JSON text the same way. An
input-limit finding's message instead states the limit reached, as in
`the document's 33554433 bytes exceed the 33554432-byte admission limit`, and
names validating again with `validate_document_with_limits` and a larger
setting, which works for validated, discovered and published bytes alike.

| Code | Rule | Meaning |
| --- | --- | --- |
| `input-limit` | OBI-01 through OBI-13 | Byte, node or caller-selected depth admission limit prevented validation; the message names validating again with `validate_document_with_limits` and the larger `JsonLimits` setting (`validateDocument` with `limits` in TypeScript). |
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
| `unpaired-surrogate` | OBI-01 | A member name or string value holds an escape denoting an unpaired surrogate. |
| `schema-mismatch` | OBI-02, OBI-10 | A constraint in the derived schema or meta-schemas failed. OBI-02's fixed message is `value violates the fixed normative schema`, at the deepest checked document positions. OBI-10 names the keyword, as in `the value of the type keyword does not satisfy the JSON Schema 2020-12 meta-schema`, at that keyword's value, or says `a schema must be a JSON object or boolean` at a schema that is neither. |
| `fixed-schema-limit-or-failure` | OBI-02, OBI-10 | Fixed-schema checking could not complete. The pinned schemas compile and their checks have no work limit, so a report does not hold it today. |
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

## Conformant document reads

A conformant operation's `input()` and `output()` return its `ValueContract`s;
`operation::Invoker::new` takes `Conformant<Document>`; `with_evaluator` and
`with_validation_detail(Detail)` return `Self`. Preparation still may refuse, and prepared does not mean decidable.
`SchemaRequest::document()` exposes the conformant document revision to custom evaluators.

| Rust read | Result and presence |
| --- | --- |
| `openbindings()` | `&str`, proved supported SemVer |
| `name()`, `version()`, `description()` | `Option<&str>`, absence separate from present decoded text |
| Conformant view `description()` | `Option<&str>` |
| Conformant operation `tags()`, dependency `kinds()` | Optional exact-size iterators of `&str`; absent differs from present-empty |
| Conformant source `kind()` | `&str`; provider support is not implied |
| Conformant dependency `accepts_kind(kind)` | Takes `&str` and compares decoded text, independent of escape spelling |
| `schemas()`, `schema(key)` | Optional exact object/boolean schema views; no preparation |
| `operations()`, `resolve_operation(name)` | Retained `ConformantOperationView` vector or optional selection |
| Conformant operation `input()`, `output()` | Optional `ValueContract`; `schema()` borrows the exact schema without preparation |
| `bindings()`, `sources()`, `dependencies()` and keyed reads | Optional retained `ConformantBindingView`, `ConformantSourceView`, `ConformantDependencyView`; absent differs from present-empty |
| Conformant operation `examples()` and `example(key)` | Retained `ConformantExampleView`; example truth is not implied |

Conformant views expose the same normative fields as document views, with infallible
reads. OBI-01 establishes scalar text for every string in a conformant document.
Descriptive text, tags and kinds borrow decoded `&str`. Original spelling and
locations remain available through each view's `value()`: for example,
`operation.value().get("description").unwrap().location()` locates its description.
For exact strings from drafts or caller values, `JsonStringRef::as_str()` can return
None for an unpaired UTF-16 unit. `utf16_units()` iterates without allocating,
`json()` retains token/location access, and equality compares decoded UTF-16. `JsonRef::as_string()` returns None only for
non-strings. The existing fallible view names remain unchanged; the `Conformant`
prefix identifies proof on the individual view. The owning document state is `Conformant<Document>`.

The [TypeScript guide](../packages/typescript/README.md) describes facade ownership,
metadata, initialization and disposal. Exact member and element traversal uses
lazy disposable iterators; retain or dispose each acquired exact-value owner.

## Inspection and exact editing

Rust `Document` enumerates and looks up retained binding, source and
dependency views. `OperationView` retains its existing binding-key association,
adds tags/deprecation, and exposes named example views. Borrowed fields are tied
to their view; clone a view or retain its `JsonRef` with `to_owned()` when needed.
Namespace enumeration distinguishes absent from empty, and field accessors reject
malformed metadata with original locations. It does not assert conformance or
resolve source availability. TypeScript projects whole namespace metadata eagerly
and checks every row; a keyed view checks only its selected metadata when read.
Existing global interpretation prerequisites still apply.

Rust editors use the atomic `Document` methods described above. `EditError::code()`
returns stable identifiers: `invalid-pointer`, `missing-parent`, `missing-value`,
`not-container`, `duplicate-members`, `overlapping-edits`, `field-collision`,
`serialization`, or parser/admission codes such as `byte-limit`, `node-limit` and
`depth-limit`. Every editing method, pointer edit or typed part, returns this one
error. An exceeded limit displays as `document edit refused: the edited document
exceeds its byte limit; admit the document with a larger JsonLimits::max_bytes
(limits.maxBytes in TypeScript)`, or its node or nesting limit; TypeScript's
`EditError` gives the same code and message on every edit route, with
`partPointer` null as Rust's carries none, and the limits apply to the edited
document, not to an edit batch's own structure. A TypeScript edit row holds only
its op's members (`op`, `pointer` and `value` for `set`, `op` and `pointer` for
`remove`, `op` and `value` for a typed namespace); anything else refuses with
`invalid-edits`, and a pointer that is not a string with `invalid-pointer`. `EditError::FieldCollision { part_pointer, .. }` carries a logical pointer
into the typed part input, as TypeScript's `EditError.partPointer` does;
`InterpretationError::source_location` addresses current exact source bytes.
These coordinates serve different purposes.

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

One prepared value contract serves both calls. `validate` (and `validate_with_control`,
`validate_json`) is the cheap check: it returns `Fails` with the fixed message
“value does not satisfy the contract”, no problems, and `problems_complete =
false`. TypeScript's `validate` returns `message`, no problems and
`problemsComplete: false`.

`explain` (and `explain_with_control`; TypeScript `explain`) returns the same
verdict and, for a failure, the default evaluator's report of the library's own
first error. Messages remain value-free, and paths map to original locations.
An explanation is **library-bounded**: building it allocates in proportion to
inputs, and the default evaluator explains only values within half its
evaluation depth limit. The SDK promises nothing stronger than the library, so
keep `validate` on paths that check untrusted input. One first error does not
establish diagnostic completeness, so the flag remains false. A work cap and
cancellation can omit detail after a verdict; missing detail never weakens
failure and never counts as rule evidence.

Each problem names its failure on both sides, by exact pointer. `instance` is the
failing node's `Pointer` from the value passed to validation (the instance's root,
also when that value is a subtree of a larger text). `schema` is a
`SchemaLocation`: `resource` (`None` for the document) and an exact `pointer` from
that document's or resource's root to the failing keyword. When a keyword cannot
be mapped to an original source, it names the selected schema rather than a
guessed keyword, and no location is invented beneath a member name the evaluator
cannot spell. Byte offset, line and column come on demand from whoever holds the
text, each with one pass over the bytes before the node and nothing retained:
`JsonValue::locate(&problem.instance)` (also on `JsonRef`), `Conformant::locate`
and `ResourceSet::locate` (TypeScript `ExactJson.locate`,
`ConformantDocument.locate`, `SchemaResources.locate`). Each returns a
`SourceLocation` whose pointer is the one asked for, or `None` when it names no
node. A resource's coordinates are in its own text: a resource built from a
subtree of a larger value counts from the subtree's start, while `JsonValue::locate`
on that subtree counts in the whole text it was parsed from. An explanation
computes no position. `NoVerdict` and `UndefinedResult`
carry an optional `SchemaLocation` of the same shape.

`Pointer` holds RFC 6901 segments exactly. `as_str()` returns `None` for a
pointer through a member name with an unpaired UTF-16 unit, `utf16_units()` reads
it exactly, `from_utf16` builds one from units and `FromStr` parses escaped text;
`JsonValue::at_pointer` resolves one as `at` resolves text, and `Document` edits
address one through `PointerRef`. `Display` writes such a
unit as `\uXXXX`, and serialization writes it as a JSON escape, which JavaScript
keeps as the same string; like an exact `JsonValue` holding one, such a record
(a report, finding, outcome, problem or location) serializes to JSON text but not
into a `serde_json::Value`, so `to_value` refuses it and `json!` panics.
`SourceLocation`'s `Display` writes `pointer (line L, column C, byte B)`, or
`line L, column C, byte B` without a pointer or at a root.

A prepared value contract keeps what its own explanation reads (a compact copy of the
keywords it compiled) and never its document, supplied resources, schema space or
any position. Custom evaluators receive the call's `Detail`: with `Detail::Outcome` a failure
carries no problems and `problems_complete` is false; with `Detail::Explain` it
carries at least one problem unless detail work was cut short or found no problem
it can report in the original sources, in which case `problems_complete` is false.

## Caller-owned operation preparation

`operation::Invoker::prepare(name)` and `prepare_with_control(name, &control)`
take the invoker as `&Arc<Invoker<P>>` and return
`Result<PreparedOperation<P>, InvocationError>`. `Invoker::new` allocates nothing;
a caller that prepares holds the configured invoker in an `Arc`, and each handle
keeps one share of it: its document, provider, decoded carriers, limits and
validation detail. The handle eagerly prepares both declared directions and owns
their compiled state. It can outlive the caller's `Arc` and is `Send`, `Sync` and
`'static` when the provider is, so one handle wrapped in an `Arc` can serve
spawned tasks. The invoker is `Send` when the provider is `Send` and `Sync` when
it is `Sync`.
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
`ERR_BINDING_NOT_FOUND` before provider work. `.with_context(&context)` forwards
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


Document findings are independent of value explanations. Locations identify
original document positions or checked schema nodes. Missing detail and truncated
findings do not change validity or rule evidence. Use rule, code and evidence for
logic, not message text or finding count.
