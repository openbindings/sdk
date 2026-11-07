# Original-node Explain witnesses

Independent bounded design review passed with the compact-seed and materialization
requirements below; implementation and qualification are pending. See
`../../receipts/2026-11-02-preparation/original-explain-review/design/review.md`.
This fills
step 4 of original-evaluator-implementation.md without restoring the outgoing
compiler, projected schemas or its diagnostic APIs. Existing signatures and names
have no compatibility requirement. Original source/value meaning remains authoritative.

## Verdict first, bounded optional explanation second

The ordinary original-node run establishes the value outcome with no diagnostic
allocations. Outcome returns that result directly. Explain returns Satisfies,
Undefined and NoVerdict unchanged; only an established Fails permits a second run
of the same interpreter with witness recording enabled. No separate validation
algorithm is introduced. A later cancellation, exhausted diagnostic allowance,
unsupported origin or incomplete proof leaves the established Fails with an empty
or partial problem list and problems_complete=false.

The second pass must not obtain a fresh copy of the caller's evaluation allowance.
Expose a private remaining work/allocation allowance from the first Graph, drop
its temporary state, and initialize the diagnostic Graph with only that remainder.
Both passes still observe the same caller control. Any enclosing preparation
attempt remains the same borrowed attempt; actual meta admission currently asks
only for Outcome. Pointer/message/materialization charges also consume the
remaining diagnostic allowance. This is cumulative requested storage/work, not a
refund or a process-memory promise. An exhausted ordinary pass establishes no
verdict and never begins diagnostics.

A default unlimited caller-selected work allowance remains unlimited; existing
internal program/stack/capability bounds still apply. Do not enlarge defaults or
preserve incidental old exact-budget thresholds. Required workload gates remain.

## A witness travels only with a proved false result

Add an optional private witness seed to State for the diagnostic run. The accepted
representation is Option<Arc<WitnessSeed>>: one machine-word State field, a charged
allocation only when recording, and cheap bounded sharing through copied/memoized
states. A seed holds an original schema node, an optional static keyword name,
and the actual instance occurrence in the call's retained arena, never an interned
equal-value identity. Materialize only the seed selected by the completed root
false proof. At every completed State boundary a present seed implies concrete
FALSE; partial coordinate/refinement facts never carry seeds. No new public proof type, environment,
instance conversion, URI normalization or callback is retained in Plan.

The ordinary run always leaves the field empty. State/frame size changes must be
measured: frame reserve already uses the actual Rust size, while copied-state
reservations must still cover their actual layout. Predicate evaluation order and
all error/control checks remain unchanged; no early-return shortcut may hide a
later required inability. Primitive checks return their first established failed
keyword along with the same validity result, without allocating a diagnostic.
Only the diagnostic run turns that result into an owned witness.

Propagation follows the operator that proves false, not the first failed check
encountered anywhere:

- Primitive false/format/pattern assertions can name their actual original
  keyword; literal false names the original Boolean schema.
- Conjunction, references, dependent schemas and unconditional item/property
  applications may retain a child's witness only when that child is concretely
  false. Successful or uncertain siblings do not manufacture a failed predicate.
- anyOf with all alternatives false and oneOf with invalid match cardinality
  name the original applicator. A rejected alternative of a successful anyOf
  must never escape as the root failure's explanation.
- not fails when its child is concretely true and names the not applicator.
- A concrete if condition selects the witness from the actual then/else result.
  Symbolic correlation that proves false without a local concrete witness may
  produce no diagnostic; do not pretend the if schema is itself an assertion.
- patternProperties/additionalProperties/unevaluatedProperties/items forward
  a child witness only when the relevant guard establishes that it applies.
  contains cardinality failure names the original contains applicator instead of
  falsely blaming an individual nonmatching item.
- propertyNames failure names the containing object's instance position and the
  propertyNames keyword. A JSON member-name token is not an instance value that
  can be addressed with an ordinary value pointer.
- Certified recursion, memoization and retry may share a witness only with the
  same already-independent false State and original instance/context. A formula
  contradiction or recursive result without such evidence remains an unexplained
  failure. Witnesses never participate in formula/effect identity or validity.

The second run must itself establish Fails and carry a valid root witness before
publishing a problem. If it returns another category or is interrupted, retain the
first verdict and publish no speculative problem. At most one problem is needed
for this bounded increment, and it never claims exhaustive coverage.

## Exact, bounded locations and messages

Construct the instance pointer relative to the actual value passed to evaluation,
including when it is a subtree of a larger arena. Keep unpaired UTF-16 names and
RFC6901 escaping exact. Reuse existing bounded location-size measurement before
allocating pointers; an absolute upper bound is conservative for a relative path.
Check that the selected occurrence is in the same arena and descends from the
actual supplied root before using an ancestor-based pointer helper.
Use original source URI and source-root pointer for the actual keyword/Boolean
schema. No synthesized document/base or guessed target is permitted. Source URI,
pointer, retained witness, message and final vector capacity all require precharges.
Include temporary ancestor/path construction, not only final string lengths.
The existing unpaired-name helper is not certified by the retained-length measure:
the independent 128-segment witness requests 6,443 bytes to retain 258. Either
reserve its complete temporary segment vectors, names and conversion buffers, or
replace it with a measured direct writer that avoids them. A fixed multiplier of
retained pointer size alone is not a proof. Source URI and keyword suffix copies
need their own checked reservations.

Messages and codes come from a finite SDK-owned keyword table and contain no
instance values, authored pattern strings, unknown property names or native
compiler errors. Optional schema facts may reuse a bounded existing detail helper
only after its ownership/location/resource contract is checked; absent details are
not fabricated. Public ValueProblem remains the generic core record. The actual
first-witness API can change freely if review finds a simpler safe organization.

## Required controls before implementation qualification

Cover direct primitive and Boolean failures; nested properties/arrays; allOf;
anyOf with a failed but irrelevant alternative; oneOf zero/multiple matches; not;
concrete and uncertain conditionals; propertyNames; contains min/max/zero counts;
conditional pattern/additional/unevaluated guards; exact decimal/UTF16 names;
original external/static/dynamic/recursive references; alias chains; recursive
certificates and memo reuse; and repeated equal values at distinct occurrences.

For each diagnostic, assert the instance pointer resolves in the actual supplied
instance, the schema origin resolves in the original source, the keyword is an
assertion whose failure contributes to the root proof, and no authored data leaks
into code/message/debug formatting. Assert irrelevant failed branches never leak.

Exercise exact/one-below work/storage including the cumulative two-pass boundary,
cancellation before verdict versus after verdict and during materialization,
ignored nested exhaustion, failed pointer construction, retained-owner cleanup and
healthy retries/concurrent calls. Outcome must allocate no witnesses. Measure the
new State/frame sizes and actual allocation reservations on native/Wasm.

Retain all 1546 forced-original values, original frame/reference/syntax/pattern
controls and the unchanged frozen Explain/performance jobs. Test actual Node and
Chromium/WebKit hosts. Reconcile existing diagnostic-detail promises before the
ordinary default switch; a partial first-witness implementation alone does not
qualify all diagnostic behavior or whole-SDK completion.
