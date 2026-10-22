# OpenBindings SDK for TypeScript

Parse, validate and author OpenBindings interface documents, inspect operations
and validate their caller-facing values. This package includes the Rust engine as
WebAssembly. Applications need no Rust toolchain.

Source inspection, interface synthesis and binding/operation invocation are Rust
companion APIs. Their coverage claims and provider streams are not exported by
this TypeScript facade; the facade exposes document operations and HTTP discovery.

## First useful result

This `0.2.0-alpha.1` package is unpublished. Install a locally built archive;
registry releases use the legacy API. Build the archive from a checkout of this
repository with Rust 1.99.0 and its `wasm32-unknown-unknown` target,
wasm-bindgen-cli 0.2.129 and Node 22 ([source replay](https://github.com/openbindings/sdk/blob/v0.2.0-alpha.1/docs/REPLAY.md)
lists the toolchain):

```sh
cd packages/typescript
npm ci --ignore-scripts
npm run build:wasm
npm run build
npm pack --ignore-scripts
```

In your project, install the archive and run the packaged Node example:

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
  using prepared = setup.valueContract;
  return { operations, result: prepared.validate(input) };
}

console.log(checkInput(7));
console.log(checkInput("seven"));
```

For a browser, serve the installed package directory over HTTP and open
[examples/first-use.html](examples/first-use.html). TypeScript 5.9.3 is qualified
with target ES2022 and `"lib": ["ES2022", "DOM", "ESNext.Disposable"]`, in Node
projects too: `ESNext.Disposable` types `using`, and the declarations use web
platform types that `DOM` declares, such as `Response`, `URL`, `AbortSignal` and
`WebAssembly`. An older compiler minimum has not been qualified.

`Document.parse` retains an inspectable, editable document. `validate()` returns a
`DocumentValidation`, as Rust's `validate()` does: a `conformant` branch carrying a
`ConformantDocument`, or a version refusal or conformance report.
`validateDocument(bytes)` returns only the plain `DocumentReport`, with no proof
and nothing to dispose. Only a conformant document's operations hand out value
contracts: `resolveOperation` returns `undefined` for a missing operation, and
`input()` or `output()` returns `undefined` when no contract is declared.
`prepare()` returns a ready `PreparedValueContract` or a no-verdict refusal. Handle
each result before validating a value. Value results distinguish `satisfies`, `fails`, `undefined`
(the specification gives the value no defined validity), `no-verdict` (the SDK could
not answer) and an `input-error` when ordinary JavaScript data cannot be admitted.
`validate` reads a string as a JSON string value; check JSON text or bytes, such as
a request body, with `validateJson`, which admits them as `parseJson` does.

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
RFC 6901 and address the revision before the batch. A path may hold an unpaired
UTF-16 unit, so a finding's pointer addresses the member it reports. A batch crosses the bridge
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
  {
    op: "set-bindings",
    value: { local: { operation: "run", source: "local" } },
  },
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
`prepare({ resources })`; the SDK never fetches a missing reference. A
contract that reaches a resource that was not supplied still prepares, and a
value whose evaluation reaches that resource gets a `resource-unavailable`
no-verdict naming it. Preparation refuses a contract in which an
`unevaluatedProperties` or `unevaluatedItems` keyword's annotation sources
reach a missing resource, a reference to a fragment of a missing resource, or
a reference that identifies no schema.
As conservative preparation, it also refuses a contract that reaches a pattern
invalid as an ECMA-262 regular expression with Unicode semantics
(`schema-pattern-compilation`), or a schema that could apply again without
consuming the instance, such as one whose `$ref` names itself (`in-place-cycle`).
Retain a prepared value contract for repeated validation. Ready
does not promise a verdict for every value: a missing resource, a pattern that
needs a backtracking engine, applies a Unicode property escape or holds the
non-word boundary `\B`, and an exhausted limit can produce no-verdict. Two
further cases give no verdict whatever `limits` sets. Exact `multipleOf`
divides within a fixed arithmetic bound: when the value or the divisor has more
than 4,096 significant digits, or an exponent beyond ±20,000 once the trailing
zeros of its digits are counted in it, a division the answer needs gives the
`numeric-arithmetic-limit` no-verdict; other numeric keywords compare exactly at
any size. And a value holding an unpaired UTF-16 unit (from a JSON `\u` escape)
in a string or member name is decided by keywords that compare exactly, such as
`const`, `enum`, `required` and `uniqueItems`, while one that reads the text,
such as `minLength`, `maxLength`, `pattern`, `properties`, `patternProperties`
or `propertyNames`, gives the `lone-surrogate-instance` no-verdict when it
reaches it.
Patterns, the word boundary `\b` included, match in time linear in the
string, so no limit bounds pattern work. Replacing resources
means preparing again. No evaluation work cap applies by default, and
evaluation time can grow exponentially with some schemas (30 levels of `anyOf`
that each name the next level twice take seconds); when validating untrusted contracts or
values within a deadline, pass `limits: { evaluationSteps: n }`. Preparation time and memory grow with the schema
resources a contract reaches (10,000 supplied resources prepare in about 0.2 s
natively), and the SDK sets no cap by default. When preparing untrusted
documents or resources, pass `limits: { schemaResources: n }`; a contract
reaching more than `n` refuses with the `schema-resource-limit` no-verdict
before the library compiles it, which bounds the time and memory of that
compilation: natively, a 7 MB document reaching a chain of 99,990 `$id`
resources prepared in 11.5 s at a 723 MB peak without a cap, and refused in
1.2 s at a 502 MB peak with a cap of 1,000.

The evaluator bounds what the SDK itself reads, writes and recurses through, not
the memory of the JSON Schema library's compiled program. On by default:
document admission limits; `stackBytes` (256 KiB), the stack the evaluator
sizes its bounds from: 256 bytes for each nested schema evaluation (1,024 at
the default), half of it for a pass of a contract holding
`unevaluatedProperties` or `unevaluatedItems` as it evaluates, and the
unevaluated walk limit below. In Wasm the guard measures the module's own
1 MiB stack, while the JavaScript engine's call stack bounds how deep the
module's calls go: Node 22 checked and explained the deepest admitted values
of the shapes probed up to `stackBytes: 524288`, and at 1 MiB a guarded
contract's deepest admitted value overflowed the engine's stack. Also on by
default: `compileJsonDepth` (512), the projected JSON nesting compiled;
`patternBytes` (1 MiB), `patternDepth` (32) and `totalPatternBytes`
(4 MiB of the forms kept for a contract's patterns);
`totalPatternMemory` (64 MiB) for a contract's compiled patterns, a quarter of
it for their search caches in equal parts and the rest for their compiled
forms, each first within 48 KiB while those that need more share what is
left, so `^.{1,10000}$` (about 13 MiB) compiles beside up to 743 small
patterns and four such counted repetitions compile together, while 40
`patternProperties` names of the form `^.{1,10000}N$` refuse with
`pattern-total-memory-limit`, whose message states the arithmetic;
`programBytes` (48 MiB) for the memory dynamic specialization adds to a
contract's program (`program-specialization-limit`); and an unevaluated
keyword's walk, refused past what `stackBytes` admits
(`unevaluated-walk-limit`). Each refusal states its arithmetic and names the
setting to raise. Within
them, preparing a schema can still take more memory than a host may want:
1,000 holders of `unevaluatedProperties` that each inherit 4,000 properties
peak at 400 MB natively. A host preparing schemas from untrusted parties should
prepare them in a Worker or process with a memory limit. Values checked against trusted
schemas stay bounded by `stackBytes` and, when set, `evaluationSteps`.

`validate` returns an established failure with no problem list and
`problemsComplete: false`. Call `explain` on the same prepared value contract for the
library's first error. An explanation allocates in proportion to input and is
made only for values within half the nesting `stackBytes` admits, so keep `validate`
for untrusted input. The problem's `instance` is the failing node's exact
pointer from the value passed, and its `schema` names the failing keyword by
`resource` and `pointer`. `value.locate(problem.instance)`,
`ConformantDocument.locate(problem.schema)` and
`SchemaResources.locate(problem.schema)` give coordinates when asked. A
prepared value contract keeps only the original locations of the schemas it
compiled, never their text, its document, resources or any position. Messages are advisory; use result tags, codes and rule evidence for
control flow. Result objects (reports, findings, outcomes, problems, locations,
refusals and discovery results) may gain properties in later versions: read the
properties you use rather than relying on an exact key set.

Document findings use exact JSON Pointers and UTF-8 byte coordinates in the assessed revision.
A pointer keeps a member name's unpaired UTF-16 unit, as a JavaScript string can,
and is null only for a syntax error. Render
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
decoded body limit. An attempt has no deadline unless you pass `timeoutMs`; a
host that needs a bound sets it. Where the host exposes
redirect responses, as Node does, the SDK checks every hop before dispatching it
and calls a supplied Fetch callback once per hop, so the callback scopes
credentials to each URL's origin; browsers follow redirects under their own
policy and the SDK checks the final URL. Only 404 means absence;
401 and 403 mean gated discovery. Dispose returned document owners. Publication
copies conformant document bytes and defaults to CORS `*`; application middleware owns
authentication and credentialed CORS.

See [HTTP policy](https://github.com/openbindings/sdk/blob/v0.2.0-alpha.1/docs/http-discovery.md),
[capabilities](https://github.com/openbindings/sdk/blob/v0.2.0-alpha.1/CAPABILITIES.md),
[migration](https://github.com/openbindings/sdk/blob/v0.2.0-alpha.1/docs/migration.md) and
[the API reference](https://github.com/openbindings/sdk/blob/v0.2.0-alpha.1/docs/api-reference.md).
