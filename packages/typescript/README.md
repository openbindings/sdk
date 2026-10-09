# OpenBindings SDK

An unpublished migration candidate with Rust semantics and a first-class
TypeScript API. Document parsing, authoring, conformance, exact values, names,
references, prepared contracts, and optional HTTP discovery are included.
Invocation, binding adaptation, and synthesis are separate work.

## Initialization

Initialization is asynchronous and explicit. Browser modules can load the Wasm
asset relative to the included module:

```ts
import { initialize, parseDocument } from '@openbindings/sdk';
await initialize();
const parsed = parseDocument(documentBytes);
if (parsed.status === 'parsed') {
  using document = parsed.value;
  const assessment = document.assess();
  // Version refusal and undetermined conformance are explicit outcomes.
}
```

Node ESM can read the included asset without a Rust toolchain:

```ts
import { readFile } from 'node:fs/promises';
import { initialize } from '@openbindings/sdk';
await initialize(await readFile(new URL(import.meta.resolve('@openbindings/sdk/openbindings.wasm'))));
```

Hosts that provide compiled Wasm imports, including workerd, can import
`@openbindings/sdk/openbindings.wasm` and pass that `WebAssembly.Module` to `initialize(module)`.
Initialize inside a request handler in hosts that restrict random-number access
to active requests. Initialization seeds the engine's randomized hash tables and
is shared across concurrent calls; subsequent requests reuse it:

```ts
import module from '@openbindings/sdk/openbindings.wasm';
import { initialize, assessDocument } from '@openbindings/sdk';
export default {
  async fetch(request: Request): Promise<Response> {
    await initialize(module);
    return Response.json(assessDocument(new Uint8Array(await request.arrayBuffer())));
  },
};
```

Missing or prohibited host entropy rejects with `host-entropy-unavailable` before
entering Rust. Retry inside a permitted context; no insecure fallback is used.
Configure the host/bundler to preserve or compile this asset. The default browser
loader needs ordinary Wasm/fetch CSP permissions. Loading failures reject with
`SdkError.code === 'wasm-initialization'`; initialization can be retried.

## Exact values and authoring

`parseJson` and `parseDocument` accept UTF-8 `Uint8Array` or JSON text. JSON tokens,
duplicate member evidence, and original source locations are retained. Locations
use zero-based byte offsets, one-based lines/byte columns, and JSON Pointers.
The text lane rejects literal unpaired UTF-16; JSON escapes such as `"\ud800"`
remain exact. Interpretation can explicitly decline unsupported string values.

`ExactJson.from` checks ordinary JavaScript values. It rejects nonfinite numbers,
undefined, bigint, functions, symbols, cycles, sparse arrays, accessors and
non-plain objects. It never silently omits a value or invokes `toJSON`. This
cannot recover precision already lost before the call. Use exact JSON text or
bytes for numbers such as `9007199254740993`.

```ts
import { authorDocument, parseJson } from '@openbindings/sdk';
const exact = parseJson('9007199254740993');
if (exact.status === 'parsed') {
  using large = exact.value;
  const authored = authorDocument({
    operations: { lookup: { input: { type: 'integer' } } },
    sources: { local: { kind: 'https://kind.example/raw', content: null } },
    additionalFields: { 'x-exact': large },
  });
  if (authored.status === 'authored') {
    using document = authored.document;
    const result = document.validate();
    if (result.status === 'validated') result.document.dispose();
  }
}
```

Typed optional authoring members use `undefined` for absence. Generic
schema/content/additional-field values reject undefined. Null remains present.
Unknown fields can be retained while conformance correctly rejects them;
additional fields cannot shadow typed members. Authoring establishes a parsed
snapshot; `validate()` establishes conformance separately. `toValue()` either
returns a checked ordinary JSON value or explicitly declines an inexact conversion.

### Recovering invalid drafts

`authorDocument` returns `authoring-error` for expected invalid draft data:

```ts
const result = authorDocument(draft);
if (result.status === 'authoring-error') {
  if (result.error.code === 'field-collision') {
    // For example: /operations/a~1~0b/additionalFields/input
    focusEditorField(result.error.draftPointer);
  }
  showDiagnostic(result.error.message);
}
```

`error.code` is a stable `AuthoringErrorCode`; `message` is explanatory text.
`draftPointer` is a JSON Pointer into the **caller's draft**, including
`additionalFields`, with `~` escaped as `~0` and `/` as `~1`. Empty string means
the draft root; null means a trustworthy draft location is unavailable. There
are no source byte offsets in an authoring failure. A cycle points to the edge
that re-enters an active ancestor; repeated acyclic objects are allowed. Sparse
arrays point to the first missing index. Symbol keys point to their containing
object. Messages omit opaque values.

| Code | Expected invalid data |
| --- | --- |
| `field-collision` | An additional field shadows a typed member. |
| `duplicate-field` | Two draft paths produce the same document member. |
| `non-finite-number` | NaN or either infinity. |
| `unsupported-value` | Undefined in opaque JSON, bigint, symbol or function. |
| `sparse-array`, `array-property` | Missing index or extra array properties. |
| `cyclic-value`, `non-plain-object` | Cycle or unsupported object prototype. |
| `accessor-property`, `non-enumerable-property`, `symbol-key` | Unsupported property representation. |
| `invalid-authoring-object` | A normative object or map is not a plain object. |
| `authoring-limit` | Ordinary conversion or encoded JSON admission limit. |
| `invalid-draft` | Rust rejects the typed draft model; its unchanged ABI does not expose a draft pointer. |

Known optional typed fields accept undefined as absence. Opaque fields do not.
Rust model refusals and encoded-input admission limits can have null pointers;
no serialized JSON offset is invented as a draft location. Accessors and `toJSON`
are never invoked on ordinary drafts. Proxy meta-object traps are outside that
ordinary-data guarantee; their failures propagate rather than being misclassified.

Initialization and disposed-handle misuse still throw structured `SdkError`s.
Unexpected engine failures also propagate. The result union is not a catch-all
for arbitrary thrown errors, and standalone `parseJson`/`ExactJson.from` contracts
are unchanged. See the runnable [editor recovery example](examples/authoring-recovery.mjs).

`ValidatedDocument` has a private instance brand. A `ParsedDocument` cannot be
assigned to it or passed to `DiscoveryPublication`: call `validate()` and narrow
on `status === 'validated'`. `retain()` on that handle preserves the validated
type. Runtime publication still verifies conformance for JavaScript and unsafe
casts; the brand is not a substitute for runtime checks.

## Inspection and contracts

`document.operations` is a cached immutable metadata snapshot. Alias lookup,
binding enumeration, dependency-kind comparison, and reference inspection do not
select or invoke anything. A contract context retains its immutable document,
supplied resources, configuration and prepared state:

```ts
using contracts = document.contracts({ resources, limits: { maxProblems: 16 } });
using input = contracts.prepare('lookup', 'input');
using value = ExactJson.from(7);
const result = input.validate(value);
// satisfies | mismatch | no-contract | operation-missing | no-verdict
```

A context caches the four most recently prepared contracts by default. Set
`cacheCapacity` to another nonnegative integer; zero disables context caching.
Eviction releases only the context's owner: a retained contract stays valid and
validates without recompilation. The entry count bounds implicit retention, not
total bytes; retain only the handles the application needs.

`SchemaResources` is immutable: `with(uri, exactDocument)` returns a new owned
set. Dispose each set when no longer needed. Resources are explicit; core never
retrieves HTTP or files. `evaluatorLimits()` describes defaults. No-verdict reasons
distinguish unsupported capability, resource absence, conservative preparation,
limits, cancellation, evaluator failure and established undefinedness.
Mismatch problems identify original schema/instance locations and omit instance
values. `problemsComplete` describes the selected diagnostic pass, not every
redundant possible failure. Truncation is explicit.

## Ownership and scheduling

Each returned handle owns its required storage. Use `using` or idempotent
`dispose()`; `retain()` creates another independently disposable owner. Retained
values and contracts survive disposal of their source handles. Using a disposed
handle throws `SdkError` with code `disposed-handle`. Garbage collection is a
fallback, not a resource-lifetime contract. The fixed meta-schema snapshots live
for the Wasm instance's lifetime; Wasm memory pages may retain allocator high-water
marks even when live owners have been released.

Parsing, assessment, inspection, preparation and validation are synchronous.
`AbortSignal` can prevent a cancelled call but cannot preempt synchronous Wasm on
the same thread. Use the included repository `examples/worker.mjs` pattern for
large CPU jobs; the owner can terminate its worker. Discovery I/O is asynchronous
and observes cancellation during a pending response read.

## HTTP discovery

Import `discover`, `DiscoveryPublication`, and `discoveryPolicy` from
`@openbindings/sdk/http-discovery` or the package root. `discover(origin, options)`
uses Fetch or a supplied local async callback. The default decoded body limit is
1 MiB, zero selects that default, and negative values are rejected. The facade
retains only bounded body bytes; Fetch controls its own stream chunk buffering.

Only 404 means absence; 401/403 are gated. Unsupported versions, nonconformance,
undetermined conformance, size limits and transport failures remain distinct.
Observable response metadata and complete bounded 200 bytes survive these
outcomes. Dispose any returned document. CORS can hide headers or the entire
response. Opaque redirects expose no final destination; `finalUrl` is null and
`responseType` records the host restriction. The acquisition URL supplies no
schema base URI.

`new DiscoveryPublication(validatedDocument, { allowOrigin: '*' })` copies the
exact approved bytes. Its `respond(request)` method returns a standard Response
for a Node or Worker adapter: GET, HEAD, 405/Allow, and 404 routing. Authentication,
credentialed CORS and preflight policy remain application responsibilities.

## Building this repository

Use the pinned Rust toolchain/wasm32 target, wasm-bindgen 0.2.129, Node 22+,
and the package lock. Run `npm ci`, `npm run build:wasm`, `npm run build`,
`npm test`, and `npm run test:types`. `WASM_BINDGEN` may name an explicit CLI path.
The build checks that generated Wasm matches current Rust source and records its
source/artifact identities. Consumers of a packed package need only its included
JavaScript, declarations and Wasm asset. The accompanying migration delivery records final source-bound qualification.
Dependency notices and compiler attribution are included; this candidate is not published.
