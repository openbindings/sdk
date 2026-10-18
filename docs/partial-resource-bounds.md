# Decisions with missing resources and defective static references

The default evaluator first prepares a complete closed program. If static reference
resolution finds an absent resource carrier, it can prepare paired validity bounds
for the fragment below. Applications still use operation contract → prepare → validate.
A prepared value contract means a validator exists, not that every value is decidable. Even a bare
missing `$ref` prepares in this fragment and returns
`NoVerdict(ResourceUnavailable)` for every admitted value.

An absent optional property can therefore satisfy its contract; a present unknown
property plus an independently failed `required` can fail; and `anyOf(true, U)`
can satisfy. If the result could depend on U, validation returns no verdict. A
missing carrier or a proven static-reference defect is a fallback trigger. Other
malformed schemas, identity ambiguity, unsupported dialects, invalid patterns, Unicode
limitations, shared limits and cancellation retain their refusal classifications. A known
error after the first hole is still checked. The strict `evaluation_program()`
helper and core dispatch to custom evaluators retain their contracts.

Prepared owners expose no resource-completeness classification. Applications retain
prepared owners and handle `NoVerdict` during validation. A prepared owner can return no
verdict for every value; a bare missing reference and an alias of one are
stage-tested, while `not` around one retains a preparation refusal. Applications
that need a reference inventory can inspect the original document and resources.
Inspection is not a proof of value decidability, and no completeness promise is
inferred for custom evaluators. Preparing with new resources does not change old
prepared owners.

When optional bounds planning declines, the default evaluator restores its original
strict located refusal detail for exactly these reason/code pairs:
ConservativePreparation with `partial-nonpositive-influence` or
`partial-annotation-or-dynamic`, and LimitExceeded with `partial-program-byte-limit`,
`partial-scratch-limit`, `schema-hole-limit`,
`partial-generated-node-limit` or `partial-generated-edge-limit`. Known-closure
defects, cancellation, known schema-node/edge/depth limits and compile failures preserve
their actual causes. This is a finite fallback policy, not a broad error-category catch.
The private planner retains its direct errors for proof tests; the evaluator alone
applies this recovery policy.

At the fixed 200,000 edge and 100,000 node caps, `schema-edge-limit` already
establishes too many known schema positions: each reached node contributes at most
one reference/hole edge, and every other edge selects a distinct child position.
Admitting 200,000 edges requires at least 100,000 such children plus the entry.
Supplying absent resources cannot make that closure fit. The actual edge refusal
is retained; it is not relabeled as a node refusal. This rationale must be revisited
if the cap relationship changes. The missing-reference cap is shadowed by the
node cap at the defaults; private small-cap tests exercise its internal boundary.

Planner refusals identify original sources where safely representable. Malformed
missing-reference fragments mark the holder's `/$ref`; unsupported keywords and
nonpositive influence mark the rejecting keyword or operand boundary. Node and
edge limits mark the position that would exceed admission, with a safely bounded
parent or selected-schema boundary if that position is too large to copy.
Aggregate projection text, generated graph and parse failures mark the selected
schema as a preparation boundary, not a uniquely offending leaf. Hole influence
starts in discovery order; known cycle checks start in node-index order so these
refusal choices are stable for fixed input/resource order.

Existing strict resolver refusals continue to mark the holder schema, including
the original `resource-unavailable` detail restored after an optional decline.
Reference facts, missing-reference evidence and malformed-fragment planner diagnostics
mark the reference keyword. Existing precise locations are preserved.

## Private evaluator contract

The bounds planner, hole influence, generated-node accounting, two-pass emission
admission, paired compilation and fallback-refusal policy belong to private
modules of the default evaluator. They are not part of the core or BYO API.
`SchemaRequest` retains the original document, supplied resources, entry, entry
location and strict `evaluation_program(control)` projection. Custom evaluators
own their preparation policy; core does not retry their refusals. Bounded source
mapping is an evaluator implementation detail; the public strict projection keeps
`original_location` for original-coordinate recovery.

**Neither bound alone is equivalent to the original partial schema.** For every
admitted value and every admissible completion that preserves known resource
identities, the Boolean ordering is:

`lower(value) ≤ actual(value) ≤ upper(value)`.

The programs make no annotation-output claim. A new resource set supplying colliding
identities or unsupported content may refuse preparation; the theorem does not
promise that every future resource set is preparable. A refused completion must
not silently invert a previously proved verdict. Old owners remain bound to their
original resource snapshot, including its holes. The SDK performs no I/O.

Missing static-reference edges use private constant targets: false for a lower
variant, true for an upper variant. Positive-only programs each need one polarity;
dependent-oneOf programs may use both internally. The original missing URI is never
registered.
Library-backed inspection exposes missing carrier identities to the planner without
copying them. Any occupied canonical private namespace segment is retained as an
integer, and the planner chooses a free namespace before projection. All generated
resources, including the constant and known-resource containers, use that namespace.
This includes relative spellings and fragments that resolve to the same carrier.
Reference siblings, property/pattern names, array prefix positions, known identities
and original mappings survive. The constants have no source map. Missing anchors
or non-schema targets in a supplied carrier are not holes. The bundled resolution
selects unique contained declared IDs before supplied names; a supplied claim on a
packaged standard identity is an identity conflict, so no completion can redefine one.
Both planners check the selected base and final target carrier for references and
every directly reached carrier. A competing canonical carrier is a conservative
`resource-identity-conflict` refusal, even through an otherwise unique retrieval
alias; it never produces an incomplete prepared owner. Independently named nested
resources retain their original base, dialect and provenance, without admitting an
unused conflicting lexical ancestor. Independently invalid pointer/anchor fragment
syntax cannot qualify an absent carrier as a hole.

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
marked, except for the separately supported dual-polarity `oneOf` transform below.
Shared aliases and advancing recursion participate in this same fixed point;
paths are not enumerated. The separate in-place cycle check remains active.
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
| Closed `not`, conditionals, `contains` | A hole-free subgraph is a fixed predicate and is kept intact. A hole influencing any of these contexts is refused, including a shared target reached by an indirect path. |

Known evaluated `unevaluatedProperties`, `unevaluatedItems`, `$dynamicRef` and
`$dynamicAnchor` reject partial planning globally. Unknown annotations could otherwise
change validity outside their own predicates or change dynamic target selection.
Closed ordinary predicates and exact literals are evaluated by the existing
backend. Advancing recursion is justified over finite admitted JSON instances:
container-child selection reduces instance depth; `propertyNames` advances to
terminal string values. The in-place graph has no admitted cycles. These facts
permit induction over instance progress and the acyclic in-place dependencies.
Finite completion tests corroborate this argument; endpoint agreement without
eligibility is not a proof (notably for mixed negation, oneOf, contains counts or
unevaluated annotations).

## Dependent oneOf

For each child, let `Li <= Ai <= Ui` bound its validity at the current instance.
Let `t` count certainly true children (`Li`) and `p` count potentially true children
(`Ui`). The actual matching count lies between `t` and `p`. The lower oneOf bound
is `t >= 1 && p == 1`; the upper bound is `p >= 1 && t <= 1`. We emit:

- Lower: `anyOf(Li) AND oneOf(Ui)`.
- Upper: `anyOf(Ui) AND oneOf([L0, ..., Ln, not(anyOf(Li))])`.

The added upper branch is true exactly when no lower child is true, so its oneOf
admits zero or one certain child. This proves the same bound inequality; oneOf is
not monotone. Uniform endpoint agreement is insufficient: `oneOf(U,V)` may pass a
mixed completion even though both uniform endpoints fail. Correlations are outside
this interval abstraction: `oneOf(U,U)` can remain no-verdict even when predicate
equality would prove failure.

Known static identities resolve before polarity selection. Required definitions
are memoized by original node and lower/upper polarity; closed nodes use one exact
variant. A dependent oneOf keeps ordinary sibling assertions in a separate body,
then conjoins the corresponding oneOf bound. Both closed registries contain the
same required definition union and distinct true/false targets, with separate
parsed arenas and different root references. No reference subtree is textually
inlined. Both copies are counted. The strict and positive-only encodings are kept.
Original in-place cycles still refuse. Generated helper references are also checked
for in-place cycles; inline glue is an acyclic tree. Polarity switches follow
original child edges, and advancing recursion retains finite-instance progress.

Memoization shares schema definitions, not evaluation results. Nested/recursive
oneOf can revisit the same lower/upper predicates, and there is no semantic result
cache. Linear emitted syntax does not establish linear runtime, low compiled
memory or acceptable latency. All visits use the same bounded verdict work/regex
allowance; exhaustion returns no verdict. Capacity and cost need separate measurement.

## Passes and diagnostics

The evaluator compiles upper first, checks cancellation, then compiles lower.
Failure of either compilation produces no prepared owner. Compilation uses the same
exact literals/numbers, pattern limits and closed no-retrieval registry as the
complete-resource path. Compilation clears the direct projection-resource owners.
Both programs name each exact `const` and `enum` value by index into one arena
that holds only those values, so the compiled constraints of both, and an
explanation, share one copy and keep no projected text alive.
Backend compilation has cooperative checks at its existing boundaries; this adds
no claim of preemptive cancellation inside a backend compile.

Upper and, when needed, lower Boolean evaluation share **one** work and regex
allowance. Cancellation is checked between passes. Upper failure proves failure;
upper success plus lower success proves satisfaction; upper success plus lower
failure returns the original missing-reference no-verdict evidence. An exhausted
or unsupported pass never establishes a verdict. With no holes both programs are
exact; disagreement is an evaluator failure, never fabricated missing evidence.

After established upper failure, an optional library-bounded first-error diagnostic pass uses only
upper. Synthetic lower/internal failures never escape. A unary
`oneOf: [{"$ref": "private upper-oneOf predicate"}]` surrounds only the complete
upper predicate of an authored oneOf, separate from ordinary sibling assertions.
Its outer failure maps exactly to the original `/oneOf` and uses a fixed,
cardinality-neutral message. It never claims which or how many authored branches
matched. An unrelated failing sibling cannot create a oneOf summary.

Private prefix, exact and synthetic-barrier entries serve the existing bounded
original-location lookup. Descendants of an exact summary stop lookup instead of
inheriting an invented original path. Offered errors without a proved origin are
omitted and mark diagnostics incomplete. The default evaluator preserves outer
`validate()` first-error boundaries and does not flatten generated contexts. The evaluator must
preserve those boundaries: a referenced target URI alone says nothing about
its dynamic occurrence in an internal circuit. Some backend structured `evaluate`
APIs drop unary wrapper events; a location map cannot reconstruct an omitted event.
Known diagnostics map to original constraints, with optional details read only
from original snapshots. A first error never claims completeness; the flag remains
false for both `validate` and `explain`. The cheap check preserves failure with empty, incomplete diagnostics. Diagnostic exhaustion never erases established failure.

## Fixed admission and ownership

The private planner admits depth 256, 100,000 reached schema nodes, 200,000 dependency edges
(including missing edges), and 100,000 missing-reference edges. Counts are checked
before graph append. Child iterators are borrowed; wide lists are not collected
before edge admission. Influence marking visits each admitted vertex/edge a bounded
number of times and uses no per-instance guard graph.

Plans with dependent oneOf additionally admit **100,000 emitted schema-node
occurrences and 200,000 reference/applicator-edge occurrences across both closed
programs**. Each emitted schema object/boolean counts as a node, including the
container, definitions, ordinary bodies, constants, inline glue, summaries and
reference wrappers. Each `$ref` and each applicator-to-child selection counts as
an edge: an applicator child `{"$ref": ...}` contributes one node and two edges.
`$defs` containment is not an evaluation edge; opaque literal/annotation JSON is
not generated schema. These units differ from source nodes and parsed JSON nodes.
Admission uses checked arithmetic before occurrence/graph append and before any
projected text or mapping-string copy. The same traversal counts and writes syntax;
all duplicated occurrences across the two programs count. The extra caps apply
only to this new fragment and are prospective, unmeasured admission choices.

The combined plan admits **64 MiB logical retained UTF-8 text**: both projected JSON
resources including generated wrappers and separately owned decoded strings,
every resource URI and entry string, generated/original mapping strings, and owned
missing-reference code/message/location text. An escaped JSON string owns its complete
decoded UTF-8 value in addition to its serialized token; both count. This applies
to member names and strings anywhere inside copied annotations or exact literals.
The two programs share one immutable source map, counted once; copied text is
counted for every retained copy. A separated literal counts, text and decoded
strings, with each program that names it, although both share its one copy. No separate hole-identity strings are retained:
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
Output separately enforces the admitted serialized and decoded sizes and checks
exact agreement at completion; dependent-oneOf output also enforces and verifies
the admitted node/edge counts. A count/output disagreement is an internal
`EvaluatorFailure` (`projection-output-mismatch`), located at the selected schema
boundary when safely representable. It is not an optional admission decline and
is never recovered to missing resources. Observed cancellation takes precedence.

A separate **64 MiB scratch-input guard** applies before invoking resolution or
copying full source pointers: reference plus base length, each full source pointer,
and each source resource URI must fit. Full pointer admission precedes any source-
prefix stripping. ResourceSet currently rebases supplied subtrees; a private source-
construction control also checks prefix stripping defensively.
Optional refusal locations are separate retained objects (including cached
preparation refusals), outside the successful paired-program text admission.
Before copying one, its full source pointer plus keyword suffix and its source URI
must each fit the scratch guard. An overlong location is omitted or replaced by a
safely bounded original boundary; pointers are never truncated and the original
reason, code and message are retained. Cancellation can remain unlocated.
Private small-cap controls cover these allocation boundaries and generated/text
admission; they do not establish production reachability. Routine native and
Node/browser checks exercise the actual node/edge caps. The ignored 64 MiB
projection stress witness remains an explicit opt-in; aggregate parse admission
and the defensive fragment UTF-8 branch have no new end-to-end witness here.
These are logical text and graph limits, not total heap, transient allocator
capacity, original caller snapshot, compiled memory or RSS promises. The existing
complete-resource projection limits and error precedence are preserved.

Partial preparation pays the repeated known scan, influence graph, paired projection
and two compiles. Upper failure uses one Boolean evaluation; success and dependent
no-verdict normally use two. These costs and final-owner release require separate
measurement alongside unchanged complete-resource baselines; semantic regression
success is not a performance result.

## Ordinary resolution and original identity

Core owns only the §7 embedding rules: the document resource and unique base,
pointers from the OBI root, document-resource plain names, contained-identity
precedence and the 2020-12 default. The internal schema adapter uses the library's
documented registry/resolver API for ordinary references within `$id` and supplied
resources. Each static lookup uses a fresh resolver. A recording `Retrieve` names
missing carriers while inspection preparation finishes; its placeholder schemas
never enter verdict compilation.

Original locations use pointer identity over the borrowed inspection resources.
The identity table is built where those inspection copies are made; it preserves numeric-copy provenance. It never uses schema equality.
ResourceSet retains supplied canonical-ID conflicts and protected standard IDs as
input policy, consulting the library's `contains_resource`. Supplied peers that
normalize to one identity refuse as `ambiguous-resource`.

OBI-13 still compares exact strings after RFC 3986 §5.2 resolution and empty-fragment
removal. Ordinary identity follows the library's normalization under §7.4, including
host case, default port and unreserved percent spellings. Encoded plain names in
`$id` and external resources follow the library's refusal. Static reference defects follow the completion rules described below; they are
not new document conformance violations.

## Inspection presentation and limits

Inspection copies now use an injective string presentation for every name:
literal U+E000 markers double, while lone surrogates have distinct marked spellings.
JSON Pointer references apply the same marker encoding, preserving other percent
and tilde escapes for the library. Both query strings and references carried in
inspection copies are translated. Provenance stays keyed by each copied node's
pointer identity, so distinct original names cannot alias one copied target.
The synthetic missing-reference root is selected deterministically outside all
normalized inspected resource names and reference carriers.

An unused malformed `$id: "%"` can still cause full-carrier preparation to refuse
a valid sibling target with `resource-inspection`. Documented borrowed/owned
registration, extension of a prepared registry, and registration of only the
desired query followed by retrieval all retain this failure. No documented
query-scoped preparation route was found. This is a conservative implementation
limit under §5.2, not an undefinedness verdict. An unused malformed `$ref` no
longer has this effect; see [Static reference defects](#static-reference-defects).

Ordinary resolution accepts `#%2F$defs%2Fgood` as another ordinary spelling
migration: inside `$id` and supplied resources the library treats the encoded
leading slash as a plain name and refuses it. The gate test remains. The document
resource still follows §7.3 decoding before classification. Probe IDs and their
the resolver regression tests cover individual outcomes.


## Static reference defects

The additional holes are **invocations**, not replacement resource bodies. They
have typed provenance from the original inspection path: a pointer reaching no
schema, an absent or duplicate literal plain name, or a malformed `$ref`
URI-reference in a resource declaring `$id`. Pointer syntax checks inspect only RFC 6901
escapes. Documented `Registry`/`Resolver::lookup` errors establish absent targets;
borrowed result identity and the existing schema-position index establish
non-schema targets. Declaration counts belong to the existing resource index.
There is no second pointer/anchor resolver. Percent-encoded anchor spelling
refusals do not establish semantic absence and keep their former refusal.

The inspection copy removes malformed static `$ref` strings at indexed schema
positions, allowing the documented library to prepare an otherwise usable
carrier. This copy never evaluates. The original invocation retains its syntax
evidence; no `$id`, anchor or ordinary reference target is reconstructed. Unused
malformed `$ref` siblings therefore cannot disable a valid target in that carrier.
Malformed `$id` siblings remain pinned conservative refusals. Supplied non-schema
embeddings use the same identity/position evidence (§7.5).

For a positive plan, replace every defective invocation with false to obtain L,
and with true to obtain H, preserving all siblings and known schema identities.
Static aliases of a hole see the same replacement. For every instance and every
admissible completion C, structural monotonicity gives L ≤ C ≤ H. The existing
positive applicators preserve this ordering; closed negative predicates are
unchanged, advancing recursion is over a finite instance, and possible in-place
cycles refuse. Annotation consumers and dynamic keywords refuse the helper.
Thus H=false proves Fails and L=true proves Satisfies. With only semantic holes,
L and H are themselves concrete admissible completions. If both evaluations
finish and L=false, H=true, they witness the two ways required by §5.2, proving
Undefined without assuming independence of repeated invocations.

Dependent `oneOf` projections prove inequalities but are not generally concrete
completions. They can still prove agreement; a split returns conservative
`partial-unproven-completions`, never Undefined. Hole influence through `not`,
conditionals or `contains` remains unqualified. Any selected closure containing
both semantic holes and missing carriers returns ResourceUnavailable; this slice
does not prove the additional universal quantifier over absent resource contents.
Compiler failures, unsupported regex, limits and cancellation remain inability.
Both witness evaluations share the existing work/regex allowance, and neither a
partial run nor a lower-bound diagnostic is used as semantic evidence.

Semantic holes share all node/edge/hole/depth/scratch admission caps. Provenance is
fixed-size enum metadata. Refusal locations identify recorded source evidence
for the closure; the completions establish dependence collectively. When the
bounds disagree on a value, an undefined result or a `resource-unavailable`
no-verdict names the reference only when the closure has one hole: its two
completions are then the two ways that reference's result comes out, so the
verdict depends on it. With several holes the evaluator does not establish which
one a value reaches, so it names none rather than one the value may never reach. The one
retained evidence location, code and message are counted before copying along with both programs and their source maps.

Durable controls are `static_reference_defects.rs`, the matching TypeScript test,
and `tools/qualification/replay/check-reference-completions.py`. They replay the
1,086-case accepted matrix on this SDK, concrete false/true/string/object
substitutions in A/L/P/D and both branch orders, and 48 semantic multi-hole groups
with sixteen completions each. The independent complete-fragment oracle checks
actual truth values; differential guards reject false agreement and require
opposite completions for reported Undefined. Injected false verdicts, reasons and
locations prove that these checks fail. Finite controls corroborate the argument;
they do not replace it. The kit and corpus replays check the supported outcomes.
