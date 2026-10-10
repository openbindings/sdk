# Decisions with missing schema resources

The default evaluator first prepares a complete closed program. If static reference
resolution finds an absent resource carrier, it can prepare paired validity bounds
for the fragment below. Applications still use context → prepare → validate.
`Ready` means a validator exists, not that every value is decidable. Even a bare
missing `$ref` prepares ready in this fragment and returns
`NoVerdict(ResourceUnavailable)` for every admitted value.

An absent optional property can therefore satisfy its contract; a present unknown
property plus an independently failed `required` can fail; and `anyOf(true, U)`
can satisfy. If the result could depend on U, validation returns no verdict. A
missing carrier is the only fallback trigger. Known malformed schemas, invalid
references/targets, ambiguity, unsupported dialects, invalid patterns, Unicode
limitations, shared limits and cancellation retain their refusal classifications. A known
error after the first hole is still checked. The strict `evaluation_program()`
helper and core dispatch to custom evaluators retain their contracts.

Prepared owners expose `resource_completeness()` in Rust and the immutable cached
`resourceCompleteness` getter in TypeScript. The default evaluator declares Complete
for supplied evaluation-relevant resources and Incomplete when bounds retain holes.
This concerns the selected contract, including transitive supplied-resource references;
unselected contracts and ignored positions do not create missing-resource evidence.
Complete does not promise a verdict for every value. Evidence names one missing
reference of this contract, not necessarily one activated by the current instance or
an exhaustive list. It contains no instance data; original locations retain the same
source-disclosure policy as diagnostics. Rust borrows evidence without cloning its
location. TypeScript caches deeply frozen plain data per owner without a new Wasm
owner, checks disposal on access, and shares cached data on retain. New contexts do
not change old prepared owners. Services/editors can require Complete before swapping;
deliberate partial previews can accept Incomplete. Custom Rust evaluators default to
Undeclared and require caller policy; core never infers completeness for them.

When optional bounds planning declines, the default evaluator restores its original
strict located ResourceUnavailable detail for exactly these reason/code pairs:
ConservativePreparation with `partial-nonpositive-influence` or
`partial-annotation-or-dynamic`, and LimitExceeded with `partial-program-byte-limit`,
`partial-scratch-limit`, `schema-edge-limit` or `schema-hole-limit`. Known-closure
defects, cancellation, shared schema-node/depth limits and compile failures preserve
their actual causes. This is a finite fallback policy, not a broad error-category catch.
The explicitly requested `evaluation_bounds()` utility retains its direct planner
errors; it neither reruns strict projection nor applies this application recovery rule.

## Adapter contract

Rust `SchemaRequest::evaluation_bounds(control)` returns an opaque owned
`EvaluationBounds`. `lower_program()` and `upper_program()` borrow closed JSON
Schema projections. `unavailable()` borrows optional original-reference refusal
evidence. `into_parts()` transfers `(lower, upper, unavailable)` without cloning
owned projection storage. No TypeScript validation variant or configuration mode
is added. Custom evaluators receive the original immutable request and can opt
into this helper; core never retries or reinterprets their refusals.

**Neither bound alone is equivalent to the original partial schema.** For every
admitted value and every admissible completion that preserves known resource
identities, the Boolean ordering is:

`lower(value) ≤ actual(value) ≤ upper(value)`.

The programs make no annotation-output claim. A new context supplying colliding
identities or unsupported content may refuse preparation; the theorem does not
promise that every future resource set is preparable. A refused completion must
not silently invert a previously proved verdict. Old owners remain bound to their
original resource snapshot, including its holes. The SDK performs no I/O.

Only missing static-reference edges are replaced, using private constant targets:
false in lower, true in upper. The original missing URI is never registered.
The existing resolver exposes missing carrier identities to the planner without
copying them. Any occupied canonical private namespace segment is retained as an
integer, and the planner chooses a free namespace before projection. All generated
resources, including the constant and known-resource containers, use that namespace.
This includes relative spellings and fragments that resolve to the same carrier.
Reference siblings, property/pattern names, array prefix positions, known identities
and original mappings survive. The constants have no source map. Missing anchors
or non-schema targets in a supplied carrier are not holes. Independently invalid
pointer/anchor fragment syntax cannot qualify an absent carrier as a hole.

## Influence proof

Each unresolved reference invocation is an arbitrary unknown predicate at its
instance location. The same URI can yield different truths at different values;
the proof never treats it as a single global Boolean variable. Its arbitrary
interior is opaque, including any unsupported outcome it might produce. Replacing
each invocation pointwise with false/true bounds every interpretation of those
outcomes. Correlations between invocations only reduce the admissible completions.

A finite typed dependency graph includes every potentially evaluated child and
static reference, including advancing child edges. A reverse queue marks all nodes
that can reach a hole. A nonpositive dependency is rejected when its target is
marked. Shared aliases and advancing recursion participate in this same fixed
point; paths are not enumerated. The separate in-place cycle check remains active.
`$defs` containment and opaque annotations are not evaluation edges. Ignored
`then`/`else` without `if`, legacy keywords and `contentSchema` remain ignored.

| Context carrying hole influence | Monotonicity argument |
| --- | --- |
| Static `$ref` | Applies its target at the same instance; target ordering is preserved. Sibling assertions remain fixed conjuncts. |
| Ordinary schema object | Keywords combine conjunctively for validity. Fixed assertions cannot reverse ordering. |
| `allOf` | Conjunction preserves each operand's ordering. |
| `anyOf` | Disjunction preserves each operand's ordering. |
| `properties` | Selection is by the original declared names and member presence. Every selected validation is a conjunct. |
| `patternProperties` | Selection is fixed by the original patterns and property names. Every selected validation is a conjunct. |
| `additionalProperties` | In an otherwise passing object, selection is equivalent to names absent from original `properties` and all original pattern matches. If a selected property/pattern already fails, the whole conjunction fails. Preserving declarations therefore preserves a monotone fixed-selection interpretation. |
| `propertyNames` | Each original property name is selected independently of the unknown result; their validations conjoin. |
| `dependentSchemas` | Trigger selection depends only on instance member presence; selected schema predicates conjoin. |
| `prefixItems` | Original prefix positions select fixed indices. Selected predicates conjoin. |
| `items` | In an otherwise passing prefix, indices start after the original prefix length. Prefix failure already fails the conjunction. Preserving every prefix slot gives monotone fixed selection. |
| Closed `not`, `oneOf`, conditionals, `contains` | A hole-free subgraph is a fixed predicate and is kept intact. A hole influencing any of these contexts is refused, including a shared target reached by an indirect path. |

Known evaluated `unevaluatedProperties`, `unevaluatedItems`, `$dynamicRef` and
`$dynamicAnchor` reject this helper globally. Unknown annotations could otherwise
change validity outside their own predicates or change dynamic target selection.
Closed ordinary predicates and exact literals are evaluated by the existing
backend. Advancing recursion is justified over finite admitted JSON instances:
container-child selection reduces instance depth; `propertyNames` advances to
terminal string values. The in-place graph has no admitted cycles. These facts
permit induction over instance progress and the acyclic in-place dependencies.
Finite completion tests corroborate this argument; endpoint agreement without
eligibility is not a proof (notably for mixed negation, oneOf, contains counts or
unevaluated annotations).

## Passes and diagnostics

The evaluator compiles upper first, checks cancellation, then compiles lower.
Failure of either compilation produces no ready owner. Compilation uses the same
exact literals/numbers, pattern limits and closed no-retrieval registry as the
complete-resource path. Projection resource arenas are released after compilation.
Backend compilation has cooperative checks at its existing boundaries; this adds
no claim of preemptive cancellation inside a backend compile.

Upper and, when needed, lower Boolean evaluation share **one** work and regex
allowance. Cancellation is checked between passes. Upper failure proves failure;
upper success plus lower success proves satisfaction; upper success plus lower
failure returns the original missing-reference no-verdict evidence. An exhausted
or unsupported pass never establishes a verdict. With no holes both programs are
exact; disagreement is an evaluator failure, never fabricated missing evidence.

After established upper failure, one separately bounded diagnostic pass uses only
upper. Synthetic lower failures never escape. Known diagnostics map to original
constraints, with optional details read only from original snapshots. Completeness
means the selected diagnostic pass completed; it does not promise diagnostics
about absent content. Zero diagnostic bytes preserves failure with empty,
incomplete diagnostics. Diagnostic exhaustion never erases established failure.

## Fixed admission and ownership

The helper admits depth 256, 100,000 reached schema nodes, 200,000 dependency edges
(including missing edges), and 100,000 missing-reference edges. Counts are checked
before graph append. Child iterators are borrowed; wide lists are not collected
before edge admission. Influence marking visits each admitted vertex/edge a bounded
number of times and uses no per-instance guard graph.

The combined plan admits **64 MiB logical retained UTF-8 text**: both projected JSON
resources including generated wrappers and separately owned decoded strings,
every resource URI and entry string, generated/original mapping strings, and owned
refusal code/message/location text. An escaped JSON string owns its complete
decoded UTF-8 value in addition to its serialized token; both count. This applies
to member names and strings anywhere inside copied annotations or exact literals.
The two programs share one immutable source map, counted once; copied text is
counted for every retained copy. No separate hole-identity strings are retained:
integer holder identities in the temporary graph refer to the immutable original
request during construction. The retained evidence names an original `$ref`.
A counting sink separately measures serialized and decoded bytes without copying
text. Unchanged JSON subtrees borrow decoded lengths from their existing flat node
storage. Rewritten names use the actual emitted escaping policy: a name rewritten
without escapes borrows its new source, while a name requiring any escape owns a
complete decoded copy. Generated numeric identities contain no escapes. Only a
fully admitted aggregate plan allocates projection buffers, mappings and evidence,
then parses the projections and their decoded strings. Buffer capacity uses only
the admitted serialized length. The check does not parse a speculative projection.

A separate **64 MiB scratch-input guard** applies before invoking resolution or
copying full source pointers: reference plus base length, each full source pointer,
and each source resource URI must fit. This can be narrower than the final relative
mapping length when a caller supplies a nested value. It is explicit and tested.
These are logical text and graph limits, not total heap, transient allocator
capacity, original caller snapshot, compiled memory or RSS promises. The existing
complete-resource projection limits and error precedence are preserved.

Partial preparation pays the repeated known scan, influence graph, paired projection
and two compiles. Upper failure uses one Boolean evaluation; success and dependent
no-verdict normally use two. These costs and final-owner release require separate
measurement alongside unchanged complete-resource baselines; semantic regression
success is not a performance result.
