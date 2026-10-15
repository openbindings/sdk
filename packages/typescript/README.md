# OpenBindings SDK for TypeScript

Parse, validate and author OpenBindings interface documents, inspect operations
and validate their caller-facing values. This package includes the Rust engine as
WebAssembly. Applications need no Rust toolchain.

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
import { parseDocument, type JsonInput } from "@openbindings/sdk";
import { initialize } from "@openbindings/sdk";

await initialize();

const documentText = `{
  "openbindings": "0.2.0",
  "operations": {
    "lookup": { "aliases": ["find"], "input": { "type": "integer" } }
  }
}`;

function checkInput(input: JsonInput) {
  const parsed = parseDocument(documentText);
  if (parsed.status !== "parsed") return parsed;
  using document = parsed.value;
  const checked = document.validated();
  if (checked.status !== "validated") return checked;
  using proof = checked.document;
  const operations = proof.operations;
  using context = proof.valueContracts();
  const setup = context.prepare("find", "input"); // primary name or alias
  if (setup.status !== "ready") return setup;
  using contract = setup.valueContract;
  return { operations, result: contract.validate(input) };
}

console.log(checkInput(7));
console.log(checkInput("seven"));
```

For a browser, serve the installed package directory over HTTP and open
[examples/first-use.html](examples/first-use.html). TypeScript 5.9.3 is qualified;
include `ESNext.Disposable` in the compiler `lib` setting and target ES2022 for
`using`. An older compiler minimum has not been qualified.

Parsing retains an inspectable snapshot. `validated()` returns either a validated
owner or a version refusal or conformance report. Only the validated owner creates
value contracts. Preparation distinguishes a ready contract, an absent value
contract, a missing operation and no-verdict. Handle each setup result before
validating a value. Value results distinguish `satisfies`, `fails`, `undefined`
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

`authorDocument` builds a parsed snapshot from typed fields. To edit a document,
call `toDraft()`, retain the returned editing scope, change `draft.value`, then
call `authorDocument` and validate the result. Typed fields are re-encoded;
opaque values retain exact tokens. Renaming an operation leaves reference updates
to the caller.

For a change inside an opaque object, use the tested
[inspection and editing recipe](examples/inspect-edit.ts). It traverses `members()`,
retains unchanged exact siblings, composes a replacement with `ExactJson.from`,
then assigns it to the draft. It refuses duplicates, missing members and names it
cannot represent. The changed object is encoded and parsed in full; formatting,
member order and name escaping may change. Keep caller-created exact values live
through the build and dispose them afterward.

## Limits and diagnostics

Schema resources are explicit and immutable. Supply `SchemaResources` when
creating a value-contract context; the SDK never fetches a missing reference.
Retain a ready contract for repeated validation. Ready does not promise a verdict
for every value; missing resources, unsupported capabilities and exhausted work
budgets can produce no-verdict. Replacing resources requires a new context.

Default `coarse` diagnostics return an established failure with no problem list
and `problemsComplete: false`. Opt into `diagnostics: "upstream"` on
`valueContracts` for the library's first error. That mode can allocate in proportion
to input and overflow the stack on deep values. Schema facts may disclose source
operands and retain source snapshots. Messages are advisory; use result tags,
codes and rule evidence for control flow.

Document findings use original JSON Pointers and UTF-8 byte coordinates. Render
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
copies validated bytes and defaults to CORS `*`; application middleware owns
authentication and credentialed CORS.

See [HTTP policy](https://github.com/openbindings/sdk/blob/main/docs/http-discovery.md),
[capabilities](https://github.com/openbindings/sdk/blob/main/CAPABILITIES.md),
[migration](https://github.com/openbindings/sdk/blob/main/docs/migration.md) and
[the API reference](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md).
