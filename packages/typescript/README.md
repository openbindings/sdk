# OpenBindings SDK for TypeScript

Parse, validate and author OpenBindings interface documents, inspect operations
and validate their caller-facing values. This package includes the Rust engine as
WebAssembly. Applications need no Rust toolchain.

Kind description, source pulling and binding/operation invocation are Rust
companion APIs. Their coverage claims and provider streams are not exported by
this TypeScript facade; the facade exposes document operations, value evaluation and HTTP discovery.

## First useful result

This `0.2.0-alpha.1` package is unpublished. Install a locally built archive;
registry releases use the legacy API. Build the archive from a checkout of this
repository with Rust 1.99.0 and its `wasm32-unknown-unknown` target,
wasm-bindgen-cli 0.2.129 and a Node version in `^22.23.0 || ^24.17.0 || >=26.2.0` ([source replay](https://github.com/openbindings/sdk/blob/main/docs/REPLAY.md)
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

This example uses `using`. Node 22.23.0 cannot run that syntax directly.
Save it as `first-use.mts` and compile it with the TypeScript command below.

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
  const entries = proof.operations(); // independently owned canonical entries
  let operations;
  try { operations = entries.map(entry => ({ key: entry.key, aliases: entry.aliases })); }
  finally { for (const entry of entries) entry.dispose(); }
  using operation = proof.resolveOperation("find"); // primary name or alias
  if (!operation) return { status: "operation-missing" } as const;
  using schema = operation.input();
  if (!schema) return { status: "no-input-schema" } as const;
  const setup = schema.prepare();
  if (setup.status !== "ready") return setup;
  using prepared = setup.schema;
  const result = prepared.explain(input); // narrow on result.status
  return { operations, result };
}

console.log(JSON.stringify(checkInput(7), null, 2));
console.log(JSON.stringify(checkInput("seven"), null, 2));
```

Compile and run it in your project after installing the SDK archive:

```sh
npm install --save-dev typescript@5.9.3
npx tsc --strict --target ES2022 --module NodeNext --moduleResolution NodeNext --lib ES2022,DOM,ESNext.Disposable --outDir build first-use.mts
node build/first-use.mjs
```

`evaluate` is the yes/no check. Its failure has no problems. `explain` adds
instance pointers, problem codes and available schema details.

For plain JavaScript on Node 22.23.0, use `try`/`finally` and `dispose()`, as
the shipped `.mjs` examples do. This complete example needs no compile step:

```js
import { Document, initialize } from "@openbindings/sdk";

await initialize();

function checkInput(input) {
  const parsed = Document.parse(
    '{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"],"input":{"type":"integer"}}}}',
  );
  if (parsed.status !== "parsed") return parsed;
  let proof, operation, schema, prepared;
  try {
    const checked = parsed.value.validate();
    if (checked.status !== "conformant") return checked;
    proof = checked.document;
    operation = proof.resolveOperation("find");
    if (!operation) return { status: "operation-missing" };
    schema = operation.input();
    if (!schema) return { status: "no-input-schema" };
    const setup = schema.prepare();
    if (setup.status !== "ready") return setup;
    prepared = setup.schema;
    return { result: prepared.explain(input) };
  } finally {
    prepared?.dispose();
    schema?.dispose();
    operation?.dispose();
    proof?.dispose();
    parsed.value.dispose();
  }
}

console.log(JSON.stringify(checkInput(7), null, 2));
console.log(JSON.stringify(checkInput("seven"), null, 2));
```

For a browser, serve the installed package directory over HTTP and open
[examples/first-use.html](examples/first-use.html). TypeScript 5.9.3 is qualified
with target ES2022 and `"lib": ["ES2022", "DOM", "ESNext.Disposable"]`, in Node
projects too: `ESNext.Disposable` types `using`, and the declarations use web
platform types that `DOM` declares, such as `Response`, `URL`, `AbortSignal` and
`WebAssembly`. An older compiler minimum has not been qualified.

`Document.parse` retains an inspectable, immutable document. `validate()` returns a
`DocumentValidation`, as Rust's `validate()` does: a `conformant` branch carrying a
`ConformantDocument`, or a version refusal or conformance report.
`validateDocument(bytes)` returns only the plain `DocumentReport`, with no proof
and nothing to dispose. `operations()` acquires canonical `Operation` owners in
lexical key order. Fields such as `key`, `aliases` and `tags` are direct reads;
optional fields preserve absence as `undefined`. `operation(key)` selects an exact
primary key; `resolveOperation(name)` also accepts aliases. Both return `undefined`
when absent. `input()` and `output()` acquire contextual `Schema` owners when
declared. `prepare()` returns an owned `PreparedSchema` or a structured preparation
refusal. Handle each result before
validating a value. Every result narrows on `status`. A value result's is
`satisfies`, `fails`, `undefined` (the specification gives the value no defined
validity), `no-verdict` (the SDK could not answer) or `input-error` when ordinary
JavaScript data cannot be admitted.
`evaluate` reads a string as a JSON string value; check JSON text or bytes, such as
a request body, with `evaluateJson`, which admits them as `parseJson` does.
Admission uses the default limits and precedes cancellation observation; malformed
JSON remains an `input-error` even when the signal is already aborted.

## Browser resource management

Use `using` or `try`/`finally` with `dispose()` for timely release. Methods such as
`value()`, `schemas()`, `input()` and `output()` acquire independent owners. Dispose
each returned owner, including every element of a domain collection and every
value in a schema map. Scalar field getters and frozen string arrays return plain data. `retain()` acquires another owner when work must outlive a scope.

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

The package runs on Node `^22.23.0 || ^24.17.0 || >=26.2.0`, the first releases whose bundled
undici rejects a chunked body cut short as its connection closes
([nodejs/undici#5273](https://github.com/nodejs/undici/pull/5273)) instead of handing `discover` the prefix
as a complete body.

Call and await `initialize()` before using the SDK. In Node ESM it reads the
installed package's Wasm asset independently of the working directory. Browser
modules load the asset relative to the package module. Hosts with compiled Wasm
imports pass a `WebAssembly.Module` to `initialize(module)`; the asset export is
`@openbindings/sdk/openbindings.wasm`.

Hosts that restrict entropy to active requests must initialize inside an allowed
request context. [The workerd example](examples/workerd-service.mjs) shows this.
Initialization is shared across concurrent calls. Loading and entropy failures
reject with `SdkError`, as does a WebAssembly module that is not this package's,
which is refused before it is instantiated; retry after correcting the host
condition.

## Exact values and editing

Use JSON text or UTF-8 bytes for exact numbers such as `9007199254740993`.
`parseJson` retains tokens; `ExactJson.from` checks ordinary JavaScript input and
cannot restore precision already lost to a JavaScript number. Nonfinite numbers,
undefined opaque values, bigint, cycles, sparse arrays and accessors are refused.

Edit a `Document` through `set(pointer, value)`, `remove(pointer)` or an atomic
`edit([{ op: "set", pointer, value }, { op: "remove", pointer }])` batch. Paths use
RFC 6901 and address the revision before the batch. Each edit returns a new `Document`; the original stays unchanged. Dispose each acquired owner independently. A path may hold an unpaired
UTF-16 unit, so a finding's pointer addresses the member it reports. A batch crosses the bridge
once, including nested exact values, splices once and reads the result once. `parseJson` values
keep exact decimal spellings. Untouched bytes, formatting and member order remain
exact; sibling insertions coalesce with compact punctuation. `EditError.code`
identifies a refusal and the document remains unchanged. The admission limits
the document was parsed with apply. Renaming operations leaves reference updates
to the caller.

Use `Document.create()` and exact path edits for construction. Set a whole namespace
when it is absent; edits do not fabricate intermediate containers. A single batch
can construct related namespaces while retaining unrelated bytes:

```typescript
using changed = document.edit([
  { op: "set", pointer: "/operations", value: { run: { input: true } } },
  { op: "set", pointer: "/dependencies", value: { dep: { operation: "run" } } },
  { op: "set", pointer: "/sources", value: { local: { kind: "custom" } } },
  {
    op: "set", pointer: "/bindings",
    value: { local: { operation: "run", source: "local" } },
  },
  { op: "set", pointer: "/name", value: "Example" },
]);
```

All pointers address the original revision. JSON input can contain borrowed exact
owners; callers retain and dispose them. `EditError.inputPointer` identifies a
refused ordinary input inside the edit batch, without inventing source coordinates.
The [inspection/editing example](examples/inspect-edit.ts) demonstrates exact edits
and canonical reads after conformance. `Operation`, `Source`, `Binding`, `Dependency`
and `Example` are owned entries of that proved snapshot. Relationships such as
`binding.source()` acquire another owner from the same snapshot.

`ConformantDocument` is a separate class with infallible reads and no setters.
`asDocument()` creates an independent immutable document owner. `intoDocument()` consumes the
proof: every alias of that proof becomes disposed and later reads fail lifecycle
checks. Independently retained proofs and views keep their certified revision.
`toBytes()` copies the current exact bytes. Dispose every acquired owner.

## Limits and diagnostics

`validateDocument`, `Document.parse` and `parseJson` admit 32 MiB of UTF-8,
1,000,000 nodes and 10,000 container levels by default; `validateDocument`
without limits sets no nesting cap. Pass `{ limits: { maxBytes, maxNodes,
maxDepth } }` to admit larger or smaller input, as Rust's `JsonLimits` does:
unspecified entries keep those defaults, so `validateDocument` with `limits`
applies the 10,000-level cap unless `maxDepth` raises it. Validation memory
grows with the document, so raise them only where the process has the memory.
Malformed limits or options throw `SdkError` with code `invalid-json-limits`.
A document keeps the limits it was parsed with (`limits`): its edits
apply them to the edited document, as Rust's do, whatever route writes
the value. `discover` raises only the byte limit, to `maxDocumentBytes`; a body
past the default 1,000,000 nodes comes back undetermined, and `validateDocument`
with a larger `maxNodes` admits it. `Document.create()`, `ExactJson.from`,
`evaluateJson` and a prepared schema's `evaluate` and `explain` of an ordinary
value use the defaults. To evaluate a value beyond those defaults, admit it with
`parseJson` and explicit limits, then pass the resulting `ExactJson` to `evaluate`.

## Evaluate values

External schemas are explicit and immutable. Pass `ExternalSchemas` to
`schema.prepare({ resources })`; no reference causes a fetch. A prepared schema
retains the meaning of that resource snapshot, so replacing resources requires a
new preparation. Retain the prepared owner for repeated evaluation. It survives
disposal of the handles used to create it and may retain immutable original sources.

Preparation failure is distinct from every value outcome. A ready schema may
return `satisfies`, `fails`, `undefined` or `no-verdict`. Missing content and
unsupported capabilities do not establish undefinedness; a result can still be
proved through independent assertions. A known undefined predicate participates
in applicator and annotation dependence. Required format assertion and supported
custom vocabularies use the original schema's dialect. An unavailable required
meta-schema can prevent schema admission and fail preparation before any value
is evaluated. See the
[evaluator guide](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/README.md)
for supported semantics and remaining limitations.

`prepare({ limits })` accepts these evaluator budgets; omitted fields use
`evaluatorLimits()`:

| Setting | Default | Scope |
| --- | --- | --- |
| `evaluationSdkWorkUnits` | `null` | Cooperative SDK evaluation work units; no caller-selected cap by default |
| `schemaResources` | `null` | Additional reached-resource count bound |
| `patternBytes` | 1 MiB | One pattern and its expanded form |
| `patternDepth` | 32 | Pattern group nesting |
| `totalPatternBytes` | 4 MiB | Unique normalized pattern forms across both pattern keywords |
| `totalPatternMemory` | 64 MiB | Immutable pattern-program representation |
| `evaluationSdkReservationBytes` | 48 MiB | Original cumulative SDK storage reservations; compiled peak scratch admission at defaults |

At default limits, Outcome checks may use a compiled jsonschema lane. Changing
any evaluator limit selects Original; `explain` always uses Original. Compiled
checks observe cancellation before and after the synchronous backend call.
See the [execution policy](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/README.md#execution-paths).

Each Original value invocation starts a fresh evaluation allowance. Verdict and diagnostic
passes share what remains; diagnostic exhaustion preserves an established failure
with incomplete problems. Required custom-meta evaluation debits its own evaluation
allowance and the shared preparation allowance, so either exhaustion can return a
preparation failure. Reservation bytes are SDK estimates, not exact allocator
requests, live heap or immutable pattern-program size.

The original schema environment also has preparation footprint, work and
allocation bounds, using Rust's default `PreparationLimits` in this facade.
Those preparation settings are not currently exposed as TypeScript options.
Original evaluation work covers traversal, predicate work and nontrivial logical reasoning;
simple Boolean identities add no proof-work unit. Growing loops retain cooperative
checks, and fast results do not reset shared preparation budgets.
Budgets are not deadlines or process-memory measurements. Original uses
original schema locations and explicit evaluation frames, with a fixed supported
maximum of 64 distinct symbolic effects per proof. The retired `stackBytes` and
`compileJsonDepth` options are rejected. Host-specific stack and performance
qualification remains part of the alignment work. Exact `multipleOf`
retains an arithmetic capability bound. Exact equality preserves unpaired UTF-16
units, while unsupported Unicode-text operations return a typed inability.

`evaluate` requests the outcome. An established failure has no problems and
`problemsComplete: false`. `explain` collects established failing assertions
at original locations, with additional allocation and work, up to 100 problems.
`problemsComplete` is true only when all reportable failures were collected;
missing witnesses, cancellation or exhausted diagnostic limits leave it false
without changing the failure verdict. `allOf` collects problems from its failing
branches. Failed `anyOf`, `oneOf` and `not` report the applicator itself, without
listing branch assertions as independent problems. `details` contains schema
facts: missing required member names, expected types, numeric and size bounds,
and complete enum choices. Bounds and choices are exact original JSON token
strings; do not convert them to `Number`. Tied missing-member problems retain
required-array order. Keywords without a matching variant, such as `const`,
`pattern`, `multipleOf` and `additionalProperties`, have no details; pattern text
stays out. Facts that cannot fit the diagnostic storage allowance or be
represented exactly are omitted and leave `problemsComplete` false. Enum lists
are never partial. Messages stay fixed SDK text. Rejected instance values are
never copied into facts or messages. Rust `Debug` remains redacted.

Serializing whole problems into a public response sends schema facts. An app
with a private contract should forward only `instance` and `code`.

A problem's `instance` is relative to the exact value passed;
`schema.sourceUri` and `schema.pointer` identify the original schema source.
`value.locate(problem.instance)`, `ConformantDocument.locate(problem.schema)` and
`ExternalSchemas.locate(problem.schema)` compute source coordinates on demand.
Keep the corresponding source owner when coordinate lookup is needed.
Messages are advisory; use result tags, codes and rule evidence for control flow.
Result records may gain properties: read the properties needed by the application.

Document findings use exact JSON Pointers and UTF-8 byte coordinates in the assessed revision.
A pointer keeps a member name's unpaired UTF-16 unit, as a JavaScript string can,
and is null for parser syntax errors and admission limits. Render
pointers as text and convert byte offsets before selecting text in a JavaScript
editor. OBI-02 findings carry `keyword`, `schema: SchemaOrigin` and optional
`details: ValueProblemDetails` from the published fixed schema. Its `sourceUri` is
`https://openbindings.com/schema/openbindings-0.2.0.json`; the pointer identifies
the failing keyword. Required details name one missing member per finding in
schema-array order, and type details list all expected types. Disallowed members
use the finding pointer and `additionalProperties`, with no rejected name or value
in details. Failed `anyOf` and `oneOf` stay at their applicator without branch
facts. The pinned OBI-02 schema contains `anyOf` only. `pattern` and `uniqueItems`
also have no detail variant. Numeric/size bounds and complete enum choices use
exact schema JSON token strings. Core does not evaluate example or caller values.

Admission limits emit `byte-limit`, `depth-limit` or `node-limit` for every rule,
with the parser offset and null pointer. Messages contain plain limit explanations,
without API advice. To admit more, call `validateDocument` with a larger
`limits.maxBytes`, `limits.maxDepth` or `limits.maxNodes`, respectively.

Findings are capped at 4,096 and 8 MiB of source pointers. The pinned 0.2 schema's
largest serialized `[keyword, schema, details]` tuple is 198 bytes in TypeScript
JSON (199 in Rust JSON). An exhaustive schema test guards this fixed property;
there is no runtime fact-size cap. `findingsTruncated` records count and
source-pointer omissions, leaving rule evidence available. The [editor example](examples/editor.html)
shows source correction, Worker scheduling and coordinate conversion.

Parsing, inspection, preparation and value validation run synchronously. An
`AbortSignal` cannot preempt same-thread Wasm. The
[browser Worker owner](examples/worker-owner.mjs) and its
[browser worker](examples/worker.mjs) demonstrate termination and recovery with
the browser Worker API.
For Node `node:worker_threads`, run the [Node parent](examples/worker-parent-node.mjs)
and [Node worker](examples/worker-node.mjs) pair:

```sh
node node_modules/@openbindings/sdk/examples/worker-parent-node.mjs
```

Each worker initializes its own SDK module instance with `await initialize()`.
The Node worker checks a value and posts plain data back. SDK owners stay in
the worker and are disposed there.

The [service lifecycle example](examples/service-lifecycle.mjs) retains old work
while installing a replacement. Public TypeScript invocation, kind-description and
source-pull providers and custom evaluator callbacks are not provided.

## HTTP discovery and reference

Initialize the SDK, then discover a document at your server's origin:

```ts
import { initialize } from "@openbindings/sdk";
import { discover } from "@openbindings/sdk/http-discovery";

await initialize();
const origin = "https://example.com";
const result = await discover(origin);
try {
  switch (result.status) {
    case "found":
      console.log("found a conformant document");
      break;
    case "absent":
      console.log("absent (404)");
      break;
    case "gated":
      console.log("gated (401 or 403)");
      break;
    case "transport-error":
      console.log("transport error:", result.error.kind);
      break;
    default:
      console.log("discovery did not establish a conformant document");
  }
} finally {
  // Non-conformant and undetermined results can also own a document.
  if ("document" in result) result.document?.dispose();
}
```

Import `discover` and `DiscoveryPublication` from
`@openbindings/sdk/http-discovery`. Discovery uses Fetch with a default 1 MiB
decoded body limit. An attempt has no deadline unless you pass `timeoutMs`; a
host that needs a bound sets it. Where the host exposes
redirect responses, as Node does, the SDK checks every hop before dispatching it
and calls a supplied Fetch callback once per hop, so the callback scopes
credentials to each URL's origin; with the default policy, browsers follow
redirects under their own policy and the SDK checks the final URL. Only 404 means absence;
401 and 403 mean gated discovery. Dispose returned document owners. Publication
copies conformant document bytes and defaults to CORS `*`; application middleware owns
authentication and credentialed CORS.

HTTP is admitted only for literal IPv4 127/8 or IPv6 ::1 by default, excluding
names such as `localhost`. Pass `{ allowHttp: true }` to `discover` or
`discoveryEndpoint` to admit plain HTTP origins and HTTP-to-HTTP hops. Plain HTTP
exposes requests, credentials and documents to network observation and
modification. HTTPS-to-HTTP redirects remain refused. With this option enabled,
a browser or Worker that hides a redirect gets a `redirect-policy`
`transport-error`, without automatic following, even if earlier discoveries used
follow mode. Hosts that expose hops, including Node, check each one. Host
mixed-content, CORS and network restrictions still apply; a rejected Fetch
request returns a network `transport-error`. The default behavior is unchanged.

See [HTTP policy](https://github.com/openbindings/sdk/blob/main/docs/http-discovery.md),
[capabilities](https://github.com/openbindings/sdk/blob/main/CAPABILITIES.md),
[migration](https://github.com/openbindings/sdk/blob/main/docs/migration.md) and
[the API reference](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md).

`Document.retainedBytes()` estimates the current shared snapshot heap bytes for
source, parsed nodes and built indexes/caches. Charge clones and proofs once;
`proof.asDocument()` acquires the shared snapshot. Later lookups can grow caches,
so call again afterwards. The estimate excludes allocator overhead, separately
owned resources and validators, process-wide packaged schemas, and Wasm memory
growth. It reads existing sizes without adding parse or validation work.

Parse failures carry `error.line` and `error.byteColumn`, one-based coordinates
in original UTF-8 bytes. A byte column is not a JavaScript UTF-16 column. Only a
refusal of input that has no source text (an edit batch) reports line 1, column 1.
