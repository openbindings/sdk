# OpenBindings SDK

An unpublished development build with Rust semantics and a first-class
TypeScript API. Document parsing, authoring, conformance, exact values, names,
references, prepared contracts, and optional HTTP discovery are included.
Invocation, binding adaptation, and synthesis are separate work.

The declarations and examples are qualified with TypeScript 5.9.3. Include
`ESNext.Disposable` in your compiler `lib` configuration for disposable handles
and `using`. An older compiler minimum has not been qualified.

## First useful result

Install a locally built archive of this unpublished candidate, then run its Node
entry directly:

```sh
npm install ./openbindings-sdk-0.2.0-alpha.1.tgz
node node_modules/@openbindings/sdk/examples/first-use-node.mjs
```

For a browser, serve the installed package directory over HTTP and open
`examples/first-use.html`. Its import map resolves the package's JavaScript entry;
the included Wasm asset loads relative to that module. Both entries run the same
[small document-to-verdict example](examples/first-use.mjs) and print operation
metadata plus a successful input, a failure and an ordinary-input admission error.
Existing registry releases belong to the legacy API; they do not install this
candidate.

The progression is initialization, parsing, conformance, operation inspection,
contract preparation, then value validation. After the host initialization below,
the same progression in TypeScript is:

```ts
import { parseDocument, type JsonInput } from "@openbindings/sdk";

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
  const checked = document.validate();
  if (checked.status !== "validated") return checked;
  using proof = checked.document;
  const operations = proof.operations;
  using context = proof.contracts();
  const setup = context.prepare("find", "input"); // primary name or alias
  if (setup.status !== "ready") return setup;
  using contract = setup.contract;
  return { operations, result: contract.validate(input) };
}

console.log(checkInput(7));
console.log(checkInput("seven"));
```

Parsing preserves a document even when a field is malformed. This application
uses `validate()` to require conformance before exposing operations. A successful
validation returns an independently owned `ValidatedDocument`; `assess()` instead
returns the report without creating that owner. The [source editor](examples/editor.html)
starts with a mistaken `inputSchema` operation member, renders its actual findings,
corrects the original source to `input`, validates it, then prepares and checks an
input. Unexpected-member findings point to the original key token. Quote pointers
and render them as text, including `findingsTruncated` when true. Lines and byte
columns refer to UTF-8 source bytes, not JavaScript UTF-16 selection offsets.
Reports retain at most 4,096 findings; expanded unexpected-field pointers also
share an 8 MiB byte budget. Omitted findings set `findingsTruncated`; all 13 rule
evidence states remain available. Expanded findings can displace later ones at
the count cap.
Messages explain the problem; rule and code fields are the machine-readable
classifications. The separate [authoring recovery lesson](examples/authoring-recovery.mjs)
uses pointers into a caller's typed draft, which have no source byte coordinates.

Handle setup separately from a value's verdict:

| Stage       | Result                                                    | Caller action                                                                                             |
| ----------- | --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Parse       | `input-error`                                             | Correct the JSON or its admission problem.                                                                |
| Conformance | `version-refused` or `assessed`                           | Report the refused version or nonconformant/undetermined report.                                          |
| Prepare     | `operation-missing`, `operation-ambiguous`, `no-contract` | Correct operation selection or explicitly decide how an absent contract fits the application.             |
| Prepare     | `no-verdict`                                              | Inspect `detail`; for example, supply a missing schema resource before retrying.                          |
| Validate    | `satisfies`                                               | The value satisfies this prepared contract.                                                               |
| Validate    | `fails`                                                | Show the bounded problems and their original schema/instance locations.                                   |
| Validate    | `input-error`                                             | Correct unsupported ordinary JavaScript input; it was not evaluated.                                      |
| Validate    | `no-verdict`                                              | Report that evaluation could not establish a verdict; inspect the reason and retry only when appropriate. |

The first-use function closes its owners after each demonstration. For repeated
validation, prepare once and keep the ready contract. The [retained service](examples/service.mjs)
does that; [replacement and recovery](examples/service-lifecycle.mjs) adds explicit
resources and asynchronous request ownership after the initialization instructions.

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

### Editing an existing document

`document.toDraft()` returns `drafted` with one `OwnedDocumentDraft`, or a
structured `authoring-error`. Its `value` is an `EditableDocumentDraft`: the
normative maps and scalar lists are writable, and schemas, content, examples and
additional fields retain exact values. Build with the same `authorDocument` used
for handwritten drafts; building does not establish conformance.

```ts
const converted = document.toDraft();
if (converted.status === 'drafted') {
  using editing = converted.draft;
  editing.value.description = 'Updated description';
  const operation = editing.value.operations.lookup;
  if (operation) (operation.tags ??= []).push('reviewed');
  const built = authorDocument(editing.value);
  if (built.status === 'authored') {
    using revised = built.document;
    console.log(revised.assess());
  } else {
    console.log(built.error);
  }
}
```

Exact leaves supplied by conversion belong to the editing scope. Removing or
replacing a field does not leak its old leaf: the owner retains its acquisition
registry until disposal. Narrow a leaf with `instanceof ExactJson`, then call
`retain()` if it must survive explicit scope disposal. Abandoning an undisposed
scope does not invalidate still-reachable leaf wrappers; unreachable leaves have
their own best-effort finalizers. Use `using` or `dispose()` for timely cleanup.
Caller-inserted exact values remain
caller-owned and must be live through the build. The scoped draft root cannot be
built after disposal. Keep editing scopes short; a retained leaf may keep its
original source arena alive. Renaming operations never rewrites references.

Typed fields are re-encoded according to their declared types: for example, an
integer `preference` written as `1.0` or `1e0` becomes `1`, and `-0` becomes `0`.
Opaque exact leaves retain their original numeric tokens.

The [complete inspection/edit example](examples/inspect-edit.ts) uses a named
`EditableDocumentDraft` helper, returns plain output, and disposes every acquired
owner. Existing readonly inputs remain accepted by `authorDocument`; opaque
interiors are not promised as mutable JavaScript graphs.

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
| `invalid-draft` | Rust rejects an encoded handwritten draft; no logical draft pointer is available. |
| `invalid-field`, `duplicate-members` | Parsed-to-draft conversion cannot represent a typed field or repeated member; inspect `sourceLocation` when available. |

Known optional typed fields accept undefined as absence. Opaque fields do not.
Rust model refusals and encoded-input admission limits can have null pointers;
no serialized JSON offset is invented as a draft location. Conversion errors use
`sourceLocation` for original bytes, distinct from a handwritten `draftPointer`.
Accessors and `toJSON`
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

## Exact traversal

`exact.members()` and `exact.elements()` return lazy disposable iterators, or
`undefined` for the wrong container kind. Object members retain source order,
duplicate occurrences and exact name tokens. Each yielded member/value is an
owner; a member's `name` and `value` getters each acquire another owner.

```ts
using members = exact.members();
if (members) {
  for (using member of members) {
    using name = member.name;
    using value = member.value;
    console.log(name.text, value.text);
    break; // closes the cursor; the using scopes release the yielded owners
  }
}
```

Use `using` for both cursor and yielded items, including arrays. Exhaustion or
`return()` closes a cursor and later `next()` stays done. Disposing a live cursor
makes later `next()` throw `disposed-handle`; disposal never revives it. Manual
`next()` callers must dispose yielded owners themselves. Retained children survive
parent/cursor disposal, and an early prefix read need not materialize all children.
The shipped [prefix preview](examples/inspect-edit.ts) handles cleanup explicitly.

## Inspection and contracts

`document.operations`, `bindings`, `sources` and `dependencies` expose frozen
ordinary metadata. Optional namespaces distinguish missing (`null`) from present
empty arrays. Keyed `binding(key)`, `source(key)` and `dependency(key)` return an
independent disposable view or `undefined`; a selected operation has `metadata`,
`examples` and `example(key)`. Exact `value`, `content`, `input` and `output`
getters acquire separate owners. Missing exact fields are `undefined`; present
JSON null is an exact value. Keep each owner in its own `using` scope.

A keyed view interprets its selected row's metadata when requested. A malformed
unrelated row can make whole-namespace metadata fail while a valid selected row
remains readable. Existing global version/duplicate/unpaired-string prerequisites
still apply. A binding with a missing source remains readable; the source lookup
returns `undefined`, and conformance assessment reports the broken reference.

`document.operations` is a cached immutable metadata snapshot. Alias lookup,
binding enumeration, dependency-kind comparison, and reference inspection do not
select or invoke anything. A contract context retains its immutable document,
supplied resources, configuration and prepared state:

```ts
using contracts = document.contracts({ resources, limits: { maxProblems: 16 } });
const setup = contracts.prepare('lookup', 'input');
if (setup.status === 'ready') {
  using input = setup.contract;
  const ordinary = input.validate(7); // ValueCheck: semantic outcome or input-error
  using value = ExactJson.from(7);
  const exact = input.validate(value); // ValueOutcome: satisfies | fails | no-verdict
}
// Other setup states: no-contract | operation-missing | operation-ambiguous | no-verdict
```

`resolveOperation(name)` returns `found` with an independently owned operation,
`missing`, or `ambiguous` with lexically ordered primary keys. The returned
metadata and candidate lists are readonly. Invalid interpreted document
structures throw `SdkError` with `code === 'interpretation'` and the original
`location` when available; raw exact evidence remains accessible. Unknown names
and absent contracts remain setup outcomes, so value validation never silently
stands for missing setup.

Ordinary validation treats a string as a JSON string value. Nonfinite numbers,
undefined, cycles, sparse arrays and unsupported property representations return
`input-error` with a stable `error.code`, explanatory `message` and
`instancePointer` into the caller's value. The root pointer is `''`; `null` means
no trustworthy input location is available. Admission does not invoke getters or
`toJSON`. Caller Proxy exceptions propagate. Nested `ExactJson` owners preserve
their tokens and remain caller-owned; temporaries are released on every result.
Disposed nested owners are misuse and throw when traversal reaches them.

An initialized SDK, live prepared handle, live exact root handle, and valid work
options are checked before cancellation. A pre-cancelled call returns
`no-verdict/cancelled` before ordinary traversal. A second check after admission
releases its temporary before returning cancellation. Work options contain only
an optional genuine `AbortSignal`. Exact overloads never return `input-error`.

A context caches the four most recently prepared contracts by default. Set
`cacheCapacity` to another nonnegative integer; zero disables context caching.
Eviction releases only the context's owner: a retained contract stays valid and
validates without recompilation. The entry count bounds implicit retention, not
total bytes; retain only the handles the application needs.

`new SchemaResources([[uri, exactDocument], ...])` constructs an immutable batch.
Input handles are borrowed. Duplicate normalized URIs are rejected, even for equal
bytes. An invalid entry or throwing iterator releases the partial set; supplied
handles remain yours. `with(uri, exactDocument)` returns another owned set.
Dispose each set when no longer needed. Resources are explicit; core never
retrieves HTTP or files. `evaluatorLimits()` describes defaults. No-verdict reasons
distinguish unsupported capability, resource absence, conservative preparation,
limits, cancellation, evaluator failure and established undefinedness.
Failure problems identify original schema/instance locations and omit instance
values. `problemsComplete` describes the selected diagnostic pass, not every
redundant possible failure. Truncation is explicit.

## Ownership and scheduling

Each returned handle owns its required storage. Use `using` or idempotent
`dispose()`; where available, `retain()` creates another independently disposable
owner. Retained values and contracts survive disposal of their source handles. Using a disposed
handle throws `SdkError` with code `disposed-handle`. Garbage collection is a
fallback, not a resource-lifetime contract. The fixed meta-schema snapshots live
for the Wasm instance's lifetime; Wasm memory pages may retain allocator high-water
marks even when live owners have been released.

| Returned owner                                                     | Lifetime                                                                                                                           |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| Parsed or validated document, exact value/view | Dispose each returned handle; `retain()` creates an independent owner.                                                             |
| Selected operation (`OperationView`) | Dispose the independently returned selection. It has no `retain()` method; its `value` getter returns a separate owned `ExactJson`. |
| Binding, source, dependency or example view | Dispose each acquired view. Metadata is plain data; exact getters acquire separate owners. `retain()` retains the view. |
| Exact iterator/member | Dispose the cursor and every yielded owner. Early iteration closure releases the cursor only. |
| Owned editable draft | Dispose after the edit/build transaction. Converted exact leaves belong to this scope; independently retain leaves that must outlive it. |
| Resource set or contract context                                   | Dispose after setup when only the prepared contract is needed. Supplied resource handles are borrowed and remain caller-owned.     |
| Ready prepared contract                                            | Retain for repeated validation; it survives disposal of setup owners. Acquire a request's retained owner before its first `await`. |

`liveStorageOwners()` counts live exact-JSON **storage arenas** in the initialized
Wasm instance. Retained handles and views can share an arena, so this is not a total
of facade handles. Fixed evaluator arenas may initialize lazily and remain for the
instance's lifetime. Record that startup retention separately; for a cleanup check,
warm the complete parse/prepare/validate job, release its application owners, then
compare later release counts with that baseline. No particular baseline number is
an API guarantee. The counter is not a byte measurement, and neither it nor disposal
establishes allocator RSS or shrinking Wasm memory.

Run the complete replacement example after installation:

```sh
node node_modules/@openbindings/sdk/examples/service-lifecycle.mjs
```

It supplies integer and string schemas under the same URI in separate immutable
contexts. An old request retains its contract before awaiting its body; preparing
and committing a replacement releases only the service's previous owner. New work
uses the new context, while the old request still validates its original exact
integer. A malformed replacement leaves the service unchanged. The example also
shows pre-cancellation, healthy recovery and warmed arena-count cleanup. Its
assertions check known fixtures; an application's setup refusals still need the
result handling described above.

Parsing, assessment, inspection, preparation and validation are synchronous.
`AbortSignal` can prevent a cancelled call but cannot preempt synchronous Wasm on
the same thread. Use [the Worker and owner examples](examples/worker-owner.mjs) for large CPU jobs.
The owner terminates its Worker to interrupt work, rejects pending jobs, ignores
stale IDs, and creates a fresh Worker for recovery. Live SDK handles never cross
Worker realms. A bundler resolves bare imports in the module Worker. Discovery I/O is asynchronous
and observes cancellation during a pending response read.

## HTTP discovery

Import `discover`, `DiscoveryPublication`, and `discoveryPolicy` from
`@openbindings/sdk/http-discovery`. Both entry points share initialization,
`SdkError` identity and managed-handle ownership. `discover(origin, options)`
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

## Complete host examples

- [First use: Node](examples/first-use-node.mjs) and
  [browser](examples/first-use.html): the same complete metadata, alias and input
  verdict path in [one shared caller](examples/first-use.mjs).
- [Service lifecycle](examples/service-lifecycle.mjs): explicit same-URI resources,
  retained requests, transactional replacement, cancellation/recovery and cleanup.
- [Editor component](examples/editor.html): a mistaken source member, quoted
  diagnostics, correction, conformance proof, contract setup and owned snapshots.
  Serve the installed package directory over HTTP.
- [Worker owner](examples/worker-owner.mjs) and [Worker job](examples/worker.mjs):
  initialization, transferred input bytes, plain results, termination and recovery.
  [Component rendering](examples/worker-view.mjs) suppresses results from older edits.
- [Retained service](examples/service.mjs): setup returns a contract owner; each
  request retains before awaiting its body, and replacement releases only the
  service owner. The [Node adapter](examples/node-service.mjs) reads the exported
  Wasm; the [local workerd adapter](examples/workerd-service.mjs) initializes in
  request context. `DELETE` tears down the example service. No deployment needed.

When migrating from this candidate's earlier facade, import HTTP discovery from
its companion path, replace `ValidatedDocument.fromParsed(parsed)` with
`parsed.validate()`, and narrow selection/preparation before accessing their
owned result. Handle absent or ambiguous setup once; validate ordinary and exact
values through the same prepared contract. There is no implicit fetch of schema
resources and no deterministic garbage-collection promise.

## Building this repository

Use the pinned Rust toolchain/wasm32 target, wasm-bindgen 0.2.129, Node 22+,
and the package lock. Run `npm ci`, `npm run build:wasm`, `npm run build`,
`npm test`, and `npm run test:types`. `WASM_BINDGEN` may name an explicit CLI path.
The build checks that generated Wasm matches current Rust source and records its
source/artifact identities. Consumers of a packed package need only its included
JavaScript, declarations and Wasm asset. The bundled `dist/wasm/build-info.json`
records the engine source and artifact hashes; [component CI](https://github.com/openbindings/sdk/actions/workflows/ci.yml)
records checks for each tested commit.
Dependency notices and compiler attribution are included; this candidate is not published.

### Interpretation errors and diagnostic text

`document.contracts()` and `document.resolveOperation()` check the complete
operation namespace, including unrelated malformed operations or aliases. They
throw `SdkError` with broad `code: "interpretation"`, an optional specific
`interpretationCode` (such as `"invalid-operation-alias"`), and an original source
`location` when available. Other error categories leave `interpretationCode`
undefined. Unrelated metadata may remain nonconformant during draft evaluation;
applications choose when to require whole-document conformance.

The established value-failure tag is `"fails"`. Default messages omit reference
identifiers, rejected instance data and user-schema enum/member strings. Type
guidance uses fixed JSON type names. Structured locations and explicit reference
spellings retain source information and need an application disclosure policy
before logging. Messages are advisory; use tagged outcomes and codes for control
flow.

`NoVerdictReason` lists the reasons emitted by this version of the TypeScript
facade and its bundled Wasm engine, which are distributed together. It is not
widened to arbitrary strings. Rust's cause enums are non-exhaustive so adding a
cause does not invalidate external wildcard matches. During pre-1.0 development,
a new minor release may change source compatibility; review the changelog before
upgrading. Neither language adds an `unknown` semantic outcome.

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md).

Value diagnostics default to an aggregate `diagnosticBytes` allowance of 1 MiB of
retained UTF-8 strings (pointers, resource URIs, codes, messages and requested detail strings), alongside
`maxProblems` (default 256, minimum count allowance one). Set these in
`document.contracts({ limits: { diagnosticBytes: 65536, maxProblems: 32 } })`.
Only complete problems are retained in deterministic order. Zero or an oversized
first problem preserves `outcome: "fails"` with `problems: []` and
`problemsComplete: false`; an empty problem list is not success. Pre-verdict limits
and cancellation keep their existing no-verdict result.

This is not a heap or wire-byte cap. Compact `JSON.stringify` failure output is
bounded by `59 + 160*N + 9*B` with optional details, or `58 + 94*N + 6*B` without them UTF-8 bytes, with `N` retained problems and `B` retained
string bytes; JSON escaping and fixed framing are included. Application wrappers
and pretty printing add bytes. The engine also preflights copied diagnostic paths,
member collections and original locations; source admission, compilation,
evaluation bookkeeping and JS/Wasm transport copies have separate costs. See the
[evaluator resource contract](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/README.md)
for scratch allowances and scope.


### Requesting exact schema facts

Default messages keep schema operands private. An authoring application can opt in:

```ts
const contracts = document.contracts({ includeSchemaDetails: true });
// Reuse contracts/ready.contract normally; dispose their owners when finished.
```

`ValueProblem.details` is an optional discriminated union: `type` has `expected`
type names; `required` has the verified missing `member`; `numeric-bound` and
`size-bound` have an exact JSON number token `bound`; `enum` has complete `choices`
as exact JSON token strings. Interpret bounds with the problem's `code`.
Do not turn exact numeric strings into JavaScript numbers when precision matters.
No rejected instance data is included. Source operands can be sensitive: choose
where to render or store them deliberately.

Absent details mean disabled or unavailable. `{ kind: "truncated" }` means requested
facts did not fit, and sets `problemsComplete: false`. The evaluator reserves the
marker before admitting a base problem; if that also fails, it omits the entire
problem. Enum lists and all tokens are atomic. String-byte accounting includes
kind tags and every detail string; the wire formula above includes enum framing.

Opting in retains whole original document/resource snapshots with prepared
contracts, potentially including unrelated source members. Disposing the context
and contracts releases those owners. A new resource set does not change facts
returned by an existing prepared contract.

The delivered [editor](./examples/editor.html) sends source and selected input text
to a component-owned module Worker. Its unchecked disclosure checkbox is the
explicit opt-in. It suppresses stale results, retains the last proved snapshot,
terminates the Worker on teardown, and converts original UTF-8 byte offsets to
textarea UTF-16 coordinates before moving the caret. It renders strings with
`textContent`; the small renderer is application code, not an SDK API.

Text entry points may throw for initialization errors, wrong JavaScript argument
types or literal unpaired UTF-16 surrogates. A malformed JSON document instead
returns its documented parse outcome. Keep operational recovery around the call:

```ts
try {
  const result = assessDocument(sourceText);
  if (result.status === "input-error") showParseError(result.error);
  else showAssessment(result);
} catch (error) {
  showOperationalError(error); // retry initialization or correct the JS input
}
```
