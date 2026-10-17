# OpenBindings SDK for TypeScript

Parse, validate and author OpenBindings interface documents, inspect operations
and validate their caller-facing values. This package includes the Rust engine as
WebAssembly. Applications need no Rust toolchain.

Source inspection, interface synthesis and binding/operation invocation are Rust
companion APIs. Their coverage claims and provider streams are not exported by
this TypeScript facade; the facade exposes document operations and HTTP discovery.

## First useful result

This `0.2.0-alpha.1` package is unpublished. Install a locally built archive;
registry releases use the legacy API. Run the packaged Node example:

```sh
npm install ./openbindings-sdk-0.2.0-alpha.1.tgz
node node_modules/@openbindings/sdk/examples/first-use-node.mjs
```

The same workflow in TypeScript initializes the engine, validates a document,
resolves an alias and checks two values:

```ts
import { Document, type JsonInput } from "@openbindings/sdk";
import { initialize } from "@openbindings/sdk";

await initialize();

const documentText = `{
  "openbindings": "0.2.0",
  "operations": {
    "lookup": { "aliases": ["find"], "input": { "type": "integer" } }
  }
}`;

function checkInput(input: JsonInput) {
  const parsed = Document.parse(documentText);
  if (parsed.status !== "parsed") return parsed;
  using document = parsed.value;
  const checked = document.validate();
  if (checked.status !== "conformant") return checked;
  using proof = checked.document;
  const operations = proof.operations;
  using operation = proof.resolveOperation("find"); // primary name or alias
  if (!operation) return { status: "operation-missing" } as const;
  using contract = operation.input();
  if (!contract) return { status: "no-input-contract" } as const;
  const setup = contract.prepare();
  if (setup.status !== "ready") return setup;
  using prepared = setup.contract;
  return { operations, result: prepared.validate(input) };
}

console.log(checkInput(7));
console.log(checkInput("seven"));
```

For a browser, serve the installed package directory over HTTP and open
[examples/first-use.html](examples/first-use.html). TypeScript 5.9.3 is qualified;
include `ESNext.Disposable` in the compiler `lib` setting and target ES2022 for
`using`. An older compiler minimum has not been qualified.

`Document.parse` retains an inspectable, editable document. `validate()` returns a
`conformant` branch carrying a `ConformantDocument`, or a version refusal or
conformance report. Only a conformant document's operations hand out value
contracts: `resolveOperation` returns `undefined` for a missing operation, and
`input()` or `output()` returns `undefined` when no contract is declared.
`prepare()` returns a ready `PreparedContract` or a no-verdict refusal. Handle
each result before validating a value. Value results distinguish `satisfies`, `fails`, `undefined`
(the specification gives the value no defined validity), `no-verdict` (the SDK could
not answer) and an `input-error` when ordinary JavaScript data cannot be admitted.

## Browser resource management

Use `using` or `try`/`finally` with `dispose()` for timely release. Methods such as
`value()`, `schemas()`, `input()` and `output()` acquire independent owners. Dispose
each returned owner, including every value in a schema map. Metadata getters
return plain data. `retain()` acquires another owner when work must outlive a scope.

For `using` in a browser without `Symbol.dispose`, load the SDK from a separate
application module after this bootstrap. Static imports run before statements:

```js
if (Symbol.dispose === undefined) {
  Object.defineProperty(Symbol, "dispose", { value: Symbol("Symbol.dispose") });
}
await import("./app.js");
```

Apply the bootstrap separately in each Worker that uses `using`. The SDK leaves
global symbols to the application. Finalizers are best effort; disposal releases
owners but does not promise shrinking process memory or Wasm capacity.

## Initialization

Call and await `initialize()` before using the SDK. In Node ESM it reads the
installed package's Wasm asset independently of the working directory. Browser
modules load the asset relative to the package module. Hosts with compiled Wasm
imports pass a `WebAssembly.Module` to `initialize(module)`; the asset export is
`@openbindings/sdk/openbindings.wasm`.

Hosts that restrict entropy to active requests must initialize inside an allowed
request context. [The workerd example](examples/workerd-service.mjs) shows this.
Initialization is shared across concurrent calls. Loading and entropy failures
reject with `SdkError`; retry after correcting the host condition.

## Exact values and editing

Use JSON text or UTF-8 bytes for exact numbers such as `9007199254740993`.
`parseJson` retains tokens; `ExactJson.from` checks ordinary JavaScript input and
cannot restore precision already lost to a JavaScript number. Nonfinite numbers,
undefined opaque values, bigint, cycles, sparse arrays and accessors are refused.

Edit a `Document` through `set(pointer, value)`, `remove(pointer)` or an atomic
`edit([{ op: "set", pointer, value }, { op: "remove", pointer }])` batch. Paths use
RFC 6901 and address the revision before the batch. A batch crosses the bridge
once, including nested exact values, splices once and reads the result once. `parseJson` values
keep exact decimal spellings. Untouched bytes, formatting and member order remain
exact; sibling insertions coalesce with compact punctuation. `EditError.code`
identifies a refusal and the document remains unchanged. Existing JSON limits
apply. Renaming operations leaves reference updates to the caller.

Use `Document.create()` and typed setters such as `setOperation(key, operation)`,
`setSource`, `setBinding`, `setDependency` and `setExample` for construction.
`setOperations(operations)` commits a complete operations map once for synthesis.
The other namespaces have corresponding map setters. To construct several namespaces
with one bridge call, splice and parse, include typed maps in the same batch:

```typescript
document.edit([
  { op: "set-operations", value: { run: { input: true } } },
  { op: "set-dependencies", value: { dep: { operation: "run" } } },
  { op: "set-sources", value: { local: { kind: "custom" } } },
  { op: "set-bindings", value: { local: { operation: "run", source: "local" } } },
  { op: "set", pointer: "/name", value: "Example" },
]);
```

Each typed map receives the same input checks as its standalone setter. A failed
batch leaves the document unchanged. All pointers address the original revision. Typed parts keep the existing
`Operation`, `Source`, `Binding`, `Dependency` and `Example`
input types. Their opaque fields accept borrowed exact owners; callers retain and
dispose those owners. `EditError.partPointer` locates a refused typed part field.
The [inspection/editing example](examples/inspect-edit.ts) demonstrates both pointer
edits and typed reads without an intermediate object projection.

`ConformantDocument` is a separate class with infallible reads and no setters.
`asDocument()` creates an independent editable owner. `intoDocument()` consumes the
proof: every alias of that proof becomes disposed and later reads fail lifecycle
checks. Independently retained proofs and views keep their certified revision.
`toBytes()` copies the current exact bytes. Dispose every acquired owner.

## Limits and diagnostics

Schema resources are explicit and immutable. Pass `SchemaResources` to
`prepare({ resources })`; the SDK never fetches a missing reference. Retain a
prepared contract for repeated validation. Ready does not promise a verdict for
every value; missing resources, unsupported capabilities and exhausted work
budgets can produce no-verdict. Replacing resources means preparing again.

`validate` returns an established failure with no problem list and
`problemsComplete: false`. Call `explain` on the same prepared contract for the
library's first error. An explanation can allocate in proportion to input and
overflow the stack on deep values, so keep `validate` for untrusted input. Schema
facts may disclose source operands. A prepared contract keeps a compact copy of
the keywords it compiled for explanations, never its document or resources.
Messages are advisory; use result tags, codes and rule evidence for control flow.

Document findings use JSON Pointers and UTF-8 byte coordinates in the assessed revision. Render
pointers as text and convert byte offsets before selecting text in a JavaScript
editor. Findings are capped at 4,096 and 8 MiB of pointers; `findingsTruncated`
leaves rule evidence available. The [editor example](examples/editor.html)
shows source correction, Worker scheduling and coordinate conversion.

Parsing, inspection, preparation and value validation run synchronously. An
`AbortSignal` cannot preempt same-thread Wasm. The
[Worker owner](examples/worker-owner.mjs) demonstrates termination and recovery.
The [service lifecycle example](examples/service-lifecycle.mjs) retains old work
while installing a replacement. Public TypeScript invocation, inspection and
synthesis providers and custom evaluator callbacks are not provided.

## HTTP discovery and reference

Import `discover` and `DiscoveryPublication` from
`@openbindings/sdk/http-discovery`. Discovery uses Fetch with a default 1 MiB
decoded body limit and thirty-second attempt deadline. Only 404 means absence;
401 and 403 mean gated discovery. Dispose returned document owners. Publication
copies conformant document bytes and defaults to CORS `*`; application middleware owns
authentication and credentialed CORS.

See [HTTP policy](https://github.com/openbindings/sdk/blob/main/docs/http-discovery.md),
[capabilities](https://github.com/openbindings/sdk/blob/main/CAPABILITIES.md),
[migration](https://github.com/openbindings/sdk/blob/main/docs/migration.md) and
[the API reference](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md).
