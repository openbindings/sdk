# Capabilities and limits

The supported specification line is stable 0.2.x, assessed using revision
`2f7d754dc2da374058cd517064c17e50f7d95d99`. SDK and specification versions are
independent. Unsupported or malformed versions are explicit refusals.

| Capability | Rust native | Browser/Node/workerd TypeScript |
| --- | --- | --- |
| Exact bytes, JSON text, duplicate evidence, original locations | Yes | Yes |
| Typed authoring, all normative objects, unknown-member retention | Yes | Yes |
| All 13 document rules; parsed/conformant states | Yes | Yes |
| Names/aliases, bindings, dependency kinds, references | Yes | Yes; cached metadata |
| Explicit schema resources; retained input/output contracts | Yes | Yes |
| Custom evaluator interface and reusable qualification kit | Rust traits | Default evaluator; JS callbacks not exposed |
| Default 2020-12 evaluator, format annotations | Yes | Same semantic engine |
| Discovery client/publication | Portable transport trait; optional native client | Fetch and standard Request/Response |
| Invocation, synthesis, binding adapters, protocol clients | Outside scope | Outside scope |

## Exact input and diagnostics

The flat immutable JSON arena preserves numeric spellings, negative zero, duplicate
object members and escaped UTF-16 code units. Parse does not establish document
conformance. Default admission is 64 MiB, 10,000 JSON containers and 1,000,000 nodes.
Limits are observable errors, not silently truncated values. Structural/source
locations use JSON Pointer, zero-based UTF-8 byte offsets and one-based lines/byte
columns. Duplicate names remain available to document rules and diagnostics.

Rust authoring retains opaque values as `JsonValue`. TypeScript exact text/bytes
are the escape route for numbers JavaScript cannot represent. Checked ordinary
conversion rejects unsupported values and reports inexact conversion; it cannot
recover rounding that happened before the call. Escaped lone surrogates are carried
exactly, while the default evaluator refuses instance interpretation requiring
Unicode scalar strings. Unknown members are preserved even if conformance rejects
them. Missing, null and empty remain distinct.

## Evaluation

Resources are caller supplied and immutable. HTTP, files, `$schema`, kinds and
source content never trigger acquisition. The anonymous document resource and
original resource/dynamic scope are preserved through a private evaluator
projection. Diagnostic locations map back to original documents. Format remains
annotation; unknown keywords are carried without inventing assertions.

Default limits: 2,000,000 evaluation steps per verdict/diagnostic pass; evaluation
depth 1,024; 2,000,000 regex steps; 256 selected problems; compiler JSON depth 512;
pattern size 1 MiB; pattern nesting 256. Schema traversal admits depth 256, 100,000
reached nodes and 64 MiB of projection text. Deep const/enum literals and annotations
use exact flat values and do not inherit recursive compiler depth. Expensive
numeric divisibility has a 4,096 normalized coefficient-digit and 20,000 absolute
normalized-power admission bound. Carriage, type checks and comparisons are not
blanket refused because of that arithmetic limit. The pinned Go 4,096-token /
exponent ±10,000 arithmetic floor is qualified separately.

Selected Unicode-property regex matching, external old/custom dialects and
lone-surrogate interpretation are declared unsupported cases. Invalid patterns,
missing external resources and some in-place recursion or dominating branches can
cause conservative no-verdict; preparation failure does not prove semantic
undefinedness. Exact-case declarations and the full suite distinguish these
limits from wrong verdicts. The implementation does not promise a complete
irrelevance decision procedure for `anyOf(true, problematic-schema)`.

Failure diagnostics are bounded, omit instance values and identify original
schema/instance locations. `problems_complete`/`problemsComplete` describes the
selected diagnostic pass, not every logically redundant failed keyword. Truncation
is explicit. Callers must handle no-verdict separately from failure and success.

## Ownership, retention and cancellation

Rust handles share immutable owned storage; prepared contracts outlive source
handles. Contexts cache four most-recent contracts by default; capacity is
configurable and zero disables caching. Eviction releases the cache owner while
retained contracts stay usable. Entry count bounds implicit retention, not bytes.
Applications control retained-handle lifetimes. Native shared contexts/validation
are qualified concurrently, including same-URI resource isolation and cancelled
preparation followed by healthy reuse.

TypeScript uses `using`, `retain()` and idempotent `dispose()`. Disposed-handle
access is an explicit error. Fixed meta-schema storage lives with the Wasm instance.
Allocator RSS/linear-memory high-water marks are different from live owners.
Synchronous CPU calls cannot be preempted by same-thread AbortSignal; use a Worker
for interruptible large jobs. Work counters bound admitted processing, not a hard
wall-clock deadline. Fetch I/O supports asynchronous cancellation and timeout.

## HTTP discovery

Strict HTTP/HTTPS origins, default decoded response-body limit 1 MiB (zero selects
default; negative rejected), bounded reads and explicit redirect policy. Only 404
means absence; 401/403 are gated with observable challenges. Unsupported versions,
nonconformance, undetermined conformance, body limits and host failures remain
distinct, retaining observable metadata and complete bounded 200 bytes.

Native TLS uses caller trust configuration; tests do not modify OS trust. Browser
CORS may hide responses/headers; opaque redirects have no observable final URL.
Requested/final URLs never supply schema bases. Fetch controls its own stream-chunk
buffering. A publication copies validated bytes and handles GET/HEAD, 405/Allow and
routing; server auth, credentialed CORS and preflight remain application policy.
