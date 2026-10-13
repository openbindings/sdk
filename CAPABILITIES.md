# Capabilities and limits

The supported specification line is stable 0.2.x, assessed using revision
`2f7d754dc2da374058cd517064c17e50f7d95d99`. SDK and specification versions are
independent. Unsupported or malformed versions are explicit refusals.

| Capability | Rust native | Browser/Node/workerd TypeScript |
| --- | --- | --- |
| Exact bytes, JSON text, duplicate evidence, original locations | Yes | Yes |
| Typed authoring, all normative objects, unknown-member retention | Yes | Yes |
| All 13 document rules; parsed/conformant states | Yes | Yes |
| Names/aliases, bindings, sources, dependencies, examples, references | Retained views and borrowed exact values | Cached metadata and retained views |
| Exact object/array traversal | Borrowed iterators | Lazy disposable iterators |
| Exact parsed-document editing | Owned typed draft | Scoped editable draft; opaque values stay exact |
| Explicit schema resources; retained input/output contracts | Yes | Yes |
| Custom evaluator interface and reusable qualification kit | Rust traits | Default evaluator; JS callbacks not exposed |
| Default 2020-12 evaluator, format annotations | Yes | Same semantic engine |
| Discovery client/publication | One-hop callback and host timer; optional native client | Fetch and standard Request/Response |
| Source inspection extension role | Provisional provider trait, checked consumer, exact binding fragments | Local async Wasm qualification probe only; no public TS role API yet |
| Interface synthesis extension role | Provisional provider trait, checked document and coverage, strict recovery | Local Wasm qualification probe; no public TS role API yet |
| Binding and operation invocation | Provisional provider/consumer APIs; bounded I/O, cancellation, selection and optional value contracts | Local Wasm qualification probes; no public TS role APIs yet |

The SDK defines the three roles and provides traits for implementing providers,
checked facades for callers, and generic examples and tests. It supplies no
kind-specific instances. The separate
[OpenAPI project](https://github.com/openbindings/openapi) implements these roles
for its documented profile. Stabilization and public TypeScript role APIs remain
separate work. Local Wasm probes qualify the Rust role seams.

## Exact input and diagnostics

The flat immutable JSON arena preserves numeric spellings, negative zero, duplicate
object members and escaped UTF-16 code units. Parse does not establish document
conformance. Default parsing admission is 64 MiB, 10,000 levels of JSON container
nesting and 1,000,000 nodes. Whole-document `assess_document` uses the default byte
and node limits with no nesting cap and checks admitted schemas locally at any
depth. `assess_document_with_limits` accepts caller-selected byte, node and depth
limits. Admission precedes version selection: an admission refusal leaves every
rule inconclusive, even if a fragment appears to declare another specification line.
Limits are observable errors, not silently truncated values. Structural/source
locations use JSON Pointer, zero-based UTF-8 byte offsets and one-based lines/byte
columns. Duplicate names remain available to document rules and diagnostics.

Rust authoring retains opaque values as `JsonValue`. TypeScript exact text/bytes
are the escape route for numbers JavaScript cannot represent. Checked ordinary
conversion rejects unsupported values and reports inexact conversion; it cannot
recover rounding that happened before the call. Escaped lone surrogates are carried
exactly through whole-document assessment. Unknown members are preserved even if
conformance rejects them. Missing, null and empty remain distinct.

## Evaluation

Resources are caller supplied and immutable. HTTP, files, `$schema`, kinds and
source content never trigger acquisition. The anonymous document resource and
original resource/dynamic scope are preserved through a private evaluator
projection. Diagnostic locations map back to original documents. Format remains
annotation; unknown keywords are carried without inventing assertions.

The bundled resolver gives unique contained declared IDs priority over supplied
name associations, followed by packaged standards. Supplied aliases and declared
IDs have equal priority; peers remain ambiguous. Every reached named carrier must
own its canonical identity, including alias, pointer, direct-child and dynamic
routes. Conflicting routes conservatively refuse; unused conflicts do not poison
other contracts or independently named nested resources. Original supplied rows,
bases, dialects and diagnostic retrieval provenance remain intact. Selection occurs
before fragment lookup, with no schema equality or fragment fallback. URI comparison
retains exact spelling after RFC resolution, literal-dot and empty-fragment removal;
case/percent-encoding equivalence is not added. Trusted fixed assessment is separate.

Default limits: 2,000,000 evaluation steps per verdict/diagnostic pass; evaluation
depth 1,024; 2,000,000 regex steps; 256 selected problems; compiler JSON depth 512;
pattern size 1 MiB; pattern nesting 256. Value-contract preparation traversal admits
schema depth 256, 100,000 reached nodes and 64 MiB of projection text. Deep const/enum
literals and annotations use exact flat values and do not inherit recursive compiler
depth. Expensive numeric divisibility has a 4,096 normalized coefficient-digit and
20,000 absolute normalized-power admission bound. Carriage, type checks and
comparisons are not blanket refused because of that arithmetic limit. The pinned
Go 4,096-token / exponent ±10,000 arithmetic floor is qualified separately.

Selected Unicode-property regex matching and external old/custom dialects are
declared unsupported cases. During preparation, opaque values and annotations do
not trigger refusal for lone surrogates; reached schema strings requiring Unicode
scalar interpretation still refuse. During value validation, unsupported-string
refusal is scoped to each evaluation and occurs only when a reached keyword needs
to interpret an unpaired string, such as patterns, lengths, const/enum string
comparisons or member-name interpretation through `propertyNames`. Boolean and
type-only schemas can decide without interpreting string contents: `true` accepts
and `false` fails a lone-surrogate instance. An unsupported access during diagnostics
preserves an established failure and marks diagnostics incomplete.

Invalid patterns and some in-place recursion or dominating branches can
cause conservative no-verdict; preparation failure does not prove semantic
undefinedness. Exact-case declarations and the full suite distinguish these
limits from wrong verdicts. The implementation does not promise a complete
irrelevance decision procedure for `anyOf(true, problematic-schema)`.

The default evaluator first tries complete-resource preparation. For missing
static-reference carriers in a qualified bounds fragment, it prepares lower/upper
validity bounds. An absent optional property can satisfy, a known failed conjunct
can fail despite an active unknown, and a known passing `anyOf` branch can satisfy.
Dependent values return `ResourceUnavailable` at validation. Even a qualified bare
missing `$ref` prepares ready; ready does not promise decidability. Hole influence
through `not`, effective conditionals or `contains` refuses; closed
subgraphs remain supported. Evaluated `unevaluated*` and dynamic keywords reject
partial bounds. Known invalid targets, dialects, patterns and cycles remain refusals.

Hole-dependent `oneOf` uses finite lower/upper polarity definitions. It can decide
zero possible winners, one certain sole winner, or multiple certain winners without
supplying a missing resource. Correlated unknown branches can remain no-verdict.
Failure summaries identify the original oneOf and make no branch-count claim.

Partial admission adds 200,000 dependency edges, 100,000 missing edges and a
64 MiB combined text allowance for both programs' serialized and owned decoded
string text, URIs, entries, shared mapping and evidence. A separate 64 MiB
resolver/full-pointer scratch guard applies. Dependent-oneOf plans additionally
admit 100,000 emitted schema-node and 200,000 reference/applicator-edge occurrences
across both closed programs, including actual duplication and generated glue.
These prospective limits are unmeasured. Memoized definitions do not cache runtime
results; nested/recursive bounds can revisit predicates. These are
not compiled-memory or RSS bounds. Upper/lower share one verdict work/regex budget;
failure diagnostics retain a separate bounded pass. The
[proof and adapter contract](docs/partial-resource-bounds.md) specifies exact
eligibility, counting and limitations. Prepared owners expose evaluator-declared
resource completeness, distinct from value decidability. Rust custom evaluators
default to Undeclared. The SDK never fetches references. Supply
resources in a new context to retry; old owners keep their original snapshots.

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

Origins use WHATWG normalization and require HTTPS except normalized literal IPv4
127/8 and IPv6 `::1`. Names or DNS results do not establish that exception.
Requested metadata records the normalized URL supplied to the transport. Native
construction installs the SDK redirect policy last on the application's builder;
an already-built client requires explicit policy delegation. Native and portable
paths follow at most ten admitted 301/302/303/307/308 redirects, resolve relative
Location, and refuse every HTTPS-to-HTTP hop before dispatch. Safe native policy
also refuses A -> B -> B credential replay caused by tower-http restoring original
headers while reqwest strips credentials only on changes from the preceding origin.
Applications needing those chains must delegate policy and own credential handling.
Portable callbacks expose individual hops and supply a monotonic, wake-capable
timer; missing timers are refused before I/O.

Fetch follows redirects and checks the observable final URL for admission and an
HTTPS-to-HTTP downgrade. Browser per-hop mixed-content and private-network policy,
including its redirect limit, remain browser-owned and unobservable to the SDK.
Opaque and opaqueredirect responses are policy refusals. CORS may hide responses
and headers; Fetch controls its own stream-chunk buffering.

The default whole-attempt deadline is 30 seconds, covering connection, TLS,
headers, all redirects, body and validation without resets. Native owns a Tokio
timer, portable discovery races one host timer, and TypeScript owns its timer and
AbortController. Positive finite overrides and explicit unbounded mode are supported;
zero timeout is refused. Timeout and caller cancellation are distinct, with caller
cancellation winning a simultaneous observation. Timers cannot preempt blocking
polls or synchronous validation; boundary checks discard late success.

The decoded response-body limit defaults to 1 MiB (zero selects that byte default;
negative limits are rejected). The SDK enables no reqwest gzip, brotli, deflate or
stream features; application clients own decoding. Native `Decode` is only the
SDK's refusal of Content-Encoding left undecoded by the host. Native body-read
errors, including truncated or corrupt-coded bodies, are `BodyFailure` with
`Network` and retain upstream messages; identified deadlines yield `Timeout`.
Portable transports may report their own decoding failures. Only complete bounded
200 bodies enter assessment; partial prefixes are never retained as documents.
Only 404 means absence; 401/403 are gated with observable challenges. Unsupported
versions, nonconformance, undetermined conformance, body limits and host failures
remain distinct. Observed metadata and complete bounded bytes survive later stops.
Requested/final URLs never supply schema bases. Native TLS uses caller trust
configuration; tests use a private CA without modifying OS trust. URL admission
does not establish connection-time address restrictions, which remain host policy.

Publication copies validated bytes and handles GET/HEAD, 405/Allow and exact path
routing. It emits `Access-Control-Allow-Origin: *` by default; explicit omission
or a browser-serialized origin can be configured. Reserved `%2F` stays escaped.
Server auth, credentialed CORS and preflight remain application policy.
