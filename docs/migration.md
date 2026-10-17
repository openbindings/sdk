# Migrate from the legacy Go and TypeScript SDKs

Move document handling first, then value validation and discovery. Keep a separate
plan for callers using invocation, inspection or synthesis: those jobs have Rust
companion APIs here, while the TypeScript facade exposes document semantics,
value validation and HTTP discovery.

Use the [Rust first-use guide](rust-first-use.md) or the
[TypeScript example](../packages/typescript/README.md) as the new caller skeleton.
The Rust-backed packages are unpublished. Build an exact checkout or use local
archives through [source replay](REPLAY.md). The npm package name is also used by
the legacy implementation; a registry install alone does not select this API.

## Parse and validate a document

| Legacy job | Rust-backed SDK |
| --- | --- |
| Go `ParseDocument(data)`, `ValidateDocument(data)` and `doc.Validate()` | `Document::parse` preserves input; `validate()` returns document validation; `conformant()` yields an owner only when conformance is established. `validate_document` is the bytes-to-validation convenience function. |
| TypeScript `parseDocument(input)` returning `OBInterface`, and `validateDocument` / `validateInterface` throwing on invalid data | Await `initialize()`, narrow `Document.parse(input)` to `status === "parsed"`, then call `document.validate()` and narrow to `status === "conformant"`. `validateDocument` returns a report outcome instead of an interface object. |

Parsing here is not a conformance gate. Keep documents for editors; require
a `Conformant<Document>` before creating value contracts or publishing a document.
Handle unsupported-version refusal separately from non-conformant and undetermined
reports. Use rule evidence and codes instead of matching diagnostic prose.

Go's `json.RawMessage`-style opaque data maps to exact `JsonValue` or `JsonRef`.
Legacy TypeScript object graphs map to disposable snapshots and exact values.
Use JSON bytes/text for wide numbers; passing a previously rounded JavaScript
number cannot restore its digits. In Rust, edit `Document` directly using `set`, `remove`, `edit(&[Edit])` and typed
part setters. Consume a proof with `into_inner()` before editing it. The Rust
first-use guide shows an exact batched edit. See the TypeScript package guide for its editing API.

## Read operations and bindings

| Legacy API | Replacement |
| --- | --- |
| Go `doc.Operations`, `doc.ResolveOperation(name)`, `doc.OperationBindings(key)` | `Conformant<Document>` operation reads and `resolve_operation(name)`; use the returned operation's `key()` and `bindings()`. Document lookup returns explicit found, missing or ambiguous selection. |
| TypeScript `iface.operations`, `resolveOperation(iface, name)`, `PreparedInterface.operation(name)` | `document.operations` metadata and `resolveOperation(name)`; narrow the selection before using its operation owner. Read schema owners with `input()` and `output()`. |

Primary names and aliases resolve operations; dependency operation references are
primary keys. A missing value contract differs from a present `true`, `false` or
object schema. Dispose TypeScript operation and exact-value owners. Metadata is
plain data, while `schemas()` and schema access methods acquire new owners.

## Prepare and validate value contracts

| Legacy API | Replacement |
| --- | --- |
| Go `NewValueContractCompiler(evaluator, resources...)`, `Resolve(ctx, doc)`, `CompileInput` / `CompileOutput` | Select a `SchemaEvaluator`, create an immutable `ResourceSet`, resolve the operation on a `Conformant<Document>`, then call `input()` or `output()` and `prepare_with(&evaluator, &resources)`. |
| Go `ValueContract.Validate(ctx, value)` / `ValidateJSON(ctx, bytes)` and error classification | Admit ordinary data with `JsonValue::from_serializable` or exact bytes with `JsonValue::parse`; match `ValueOutcome::Satisfies`, `Fails`, `Undefined` or `NoVerdict`. |
| TypeScript `compileOperationSchema(iface, name, position)` and `CompiledSchema.validate(value)` returning `valid` and failures | Call `conformant.resolveOperation(name)`, then `input()` or `output()` and `prepare(options)`, narrow to ready, then call `contract.validate(value)`. Handle `satisfies`, `fails`, `undefined`, `no-verdict` and ordinary-input admission errors. |

Preparation has its own refusal. An absent value contract is not success against
an implicit schema. Retain a prepared contract for reuse; preparation compiles anew.
Resources are caller-supplied and immutable, with no automatic reference fetching.
Changing resources means preparing again; old contracts keep their snapshots.
A prepared contract may still return no-verdict for individual values. An undefined
result is its own outcome: match `ValueOutcome::Undefined { detail }` in Rust or
`{ outcome: "undefined", detail }` in TypeScript. `UndefinedResult` carries a code,
message and optional original schema location. Move handling of the former
`NoVerdictReason::Undefined` or `"undefined"` refusal reason to this branch;
no-verdict now carries only inability to answer. Legacy
`PreparedBoundaryContract.complete` and boundary-comparison helpers have no direct
replacement or decidability equivalent here.

Rust core requires an explicit operation value evaluator, including for boolean
schemas. TypeScript supplies the bundled evaluator. See
[custom evaluator integration](schema-evaluators.md). `validate` reports a failure
without problems; `explain` asks for the library's first error and accepts its
allocation and stack limits. Diagnostic count and prose are not compatibility
contracts.

## Invoke

Legacy TypeScript callers use `OperationInvoker` or
`OpenBindingsRuntime.invoke`, with provider and context selection. This facade
has no public TypeScript invocation replacement. Retain that dependency for those
jobs or move the integration to Rust.

In Rust, implement `binding::BindingInvoker` or select a kind implementation that
does. Use `binding::Invoker` for source and binding values, or
`operation::Invoker::new(conformant, provider)` for document operations. Select an
evaluator with `with_evaluator`, prepare an operation and invoke an `Attempt`.
Read output through its terminal result and close input when finished. See the
[invocation guide](invocation.md). Legacy `prepareOperation` is an advisory
context/preflight job; it is not equivalent to Rust's retained `prepare` handle.

The checked Go release line exposes no invocation package, so it has no API to
map for this job. Earlier removed Go modules require their own migration analysis.
Kind implementations and application context policy remain separate from core.

## Discover and publish over HTTP

| Legacy API | Replacement |
| --- | --- |
| Go `httpdiscovery.Client.Discover(ctx, origin)` | Rust `native::Client::discover` with the `native` feature, or portable `discover` with host transport and timer. |
| Go `httpdiscovery.NewHandler(data, options)` | Rust `Publication::from_bytes` / `respond`, adapted to your HTTP server. |
| TypeScript `fetchInterface(url)` or `OpenBindingsRuntime.resolve(url)` | `discover(origin, options)` from `@openbindings/sdk/http-discovery` for well-known discovery. `DiscoveryPublication` publishes conformant bytes. |

Legacy TypeScript resolution tries direct fetching, discovery and optional
synthesis. The new discovery call performs the well-known-origin job only;
compose direct fetching or synthesis explicitly when needed. Acquisition URLs
never become schema base URIs. The default decoded body limit is 1 MiB and the
whole-attempt deadline is thirty seconds. Match absence, gated discovery,
transport, HTTP and document outcomes separately. Publication defaults to CORS
`*`; authentication and credentialed CORS remain application policy.
Read [HTTP policy](http-discovery.md) before porting a custom transport.

## Inspect sources and synthesize interfaces

Legacy TypeScript exposes `SourceInspector.inspectSource`,
`InterfaceSynthesizer.synthesizeInterface`, the optional
`CoverageSynthesizer.synthesizeInterfaceWithCoverage`,
and corresponding `OpenBindingsRuntime` methods. They have no public TypeScript
replacement here. Rust uses `SourceInspector` with the checked `Inspector`, and
`InterfaceSynthesizer` with `Synthesizer`. Callers assign source and operation
keys, inspect coverage, and choose strict or partial-result policy. See the
[inspection guide](../crates/openbindings-source-inspection/README.md) and
[synthesis guide](../crates/openbindings-interface-synthesis/README.md).

The checked Go release line exposes neither inspection nor synthesis. Providers
must be integrated under their own kind contracts; the SDK does not infer a
universal provider registry, credential policy or supported-kind set.

## Legacy source references

The mappings above were checked against the legacy `origin/release/0.2` sources:
Go revision `b67b7d4b0f754ea6f42dffe047db8dbabdaec8eb` and TypeScript revision
`173a5b4a63a4574b9576338add880b215749a66f`.

- Go: [parsing](https://github.com/openbindings/openbindings-go/blob/b67b7d4b0f754ea6f42dffe047db8dbabdaec8eb/parse.go),
  [value contracts](https://github.com/openbindings/openbindings-go/blob/b67b7d4b0f754ea6f42dffe047db8dbabdaec8eb/value_contracts.go),
  [operation lookup](https://github.com/openbindings/openbindings-go/blob/b67b7d4b0f754ea6f42dffe047db8dbabdaec8eb/resolve_operation.go),
  [HTTP client](https://github.com/openbindings/openbindings-go/blob/b67b7d4b0f754ea6f42dffe047db8dbabdaec8eb/httpdiscovery/client.go)
  and [publication](https://github.com/openbindings/openbindings-go/blob/b67b7d4b0f754ea6f42dffe047db8dbabdaec8eb/httpdiscovery/server.go).
- TypeScript: [parsing](https://github.com/openbindings/openbindings-ts/blob/173a5b4a63a4574b9576338add880b215749a66f/packages/core/src/parse.ts),
  [schema compilation](https://github.com/openbindings/openbindings-ts/blob/173a5b4a63a4574b9576338add880b215749a66f/packages/core/src/schema-validation.ts),
  [prepared interfaces](https://github.com/openbindings/openbindings-ts/blob/173a5b4a63a4574b9576338add880b215749a66f/packages/core/src/prepared-interface.ts),
  [runtime](https://github.com/openbindings/openbindings-ts/blob/173a5b4a63a4574b9576338add880b215749a66f/packages/sdk/src/runtime.ts)
  and [fetching](https://github.com/openbindings/openbindings-ts/blob/173a5b4a63a4574b9576338add880b215749a66f/packages/synthesize/src/fetch.ts).
