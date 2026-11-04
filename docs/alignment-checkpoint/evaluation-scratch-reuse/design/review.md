# Evaluation scratch reuse: bounded design review

**Approved for the narrow implementation and qualification below.** Production
source reviewed at capacity checkpoint `a7aab1e`, with exact file hashes in this
directory. No SDK code was edited. This does not approve a G6 residual, a second
execution engine, a different exact-value model, or the remaining large identity
retention cost.

The proposal removes two concrete transient allocation patterns: each alias walk
currently creates an empty Entries owner that grows two vectors, and every
Values::intern call allocates a one-element pending vector even for a scalar or an
already interned occurrence. Neither allocation establishes a semantic fact.
Reuse/inline ownership is simpler than adjusting quota coefficients or duplicating
the scalar canonicalization algorithm.

## Alias scratch ownership

Use one private `Entries<(usize, Context), Option<String>>` field in Run. Its
logical contents must be empty outside one synchronous alias traversal. Its vector
capacities may remain until Run is dropped. No alias call recursively invokes
another alias call: the current traversal calls enter, child and prepared query
selection, not schema evaluation. A Plan never owns or shares the scratch.

Prefer a small outer wrapper and one fallible helper/closure:

1. Take the empty scratch owner from Run without allocation.
2. Execute the complete current alias traversal against the local owner, saving
   its Result. All `?` and early returns belong inside this helper.
3. Reverse-pop every inserted entry without asking the Graph for more work.
4. Restore the now-empty owner to Run, then return the saved Result.

An ordinary return must never bypass steps 3–4. On panic, the local owner drops during
unwind; no Plan receives scratch. An elaborate new guard/interner or shared mutable
cache is unnecessary. Return the already owned Context/tuple, never a borrow into
the path. Cycle suffix inspection and relevant-context construction finish before
cleanup, so the current canonical node, used dynamic names and original scope
meaning are unchanged.

Prepay the explicit cleanup loop when adding each entry, before insertion. A
failed insertion may leave unused prepaid work; it cannot leave an uncharged
successful entry. Cleanup after cancellation/exhaustion must finish and release
owned dynamic strings/context references without asking an exhausted control for
permission. Retain the existing charges for hashing/equality, context creation,
dynamic-name ownership and vector growth; this is not a blanket repricing of those
operations.

The existing Entries implementation supports the required invariant. Push records
the previous bucket head. Rehash recomputes these links in entry order. A reverse
pop restores that previous head, so emptying entries leaves every bucket empty even
after growth and collisions. Backing capacity and bucket length can stay large.
Using `clear()` instead would fill the entire retained bucket vector and make a
short later walk pay for an earlier long walk. A long-then-short control must show
cleanup proportional to live inserted entries rather than retained width.

Restoring the scratch must not reset Graph work/storage, semantic memo, active
application entries, provisional facts, effect identities or dynamic context.
This scratch is separate from the active schema stack. Outcome and optional
Explain passes keep their existing separate Runs and cumulative allowance policy.

## Inline first value work item

Keep one postorder canonicalization body. Start with
`Some((value, false))` inline and an empty pending Vec, taking the inline item
before popping the Vec. A scalar or known occurrence then needs no pending backing.
A container still pushes its expanded continuation and children in the existing
order. This is less error-prone than a second scalar matcher/interner path.

Use checked needed lengths and the existing `storage::reserve` before each actual
growth/push. An object pair must reserve room for both name and value before either
unchecked push. A parent continuation must precede its children on the stack so it
is not interned until their identities exist. Removing the one-element allocation
also removes its matching old reservation; do not retain a fictional 64-byte charge
or remove unrelated value/map/canonical-key charges. Existing ticks before occurrence
lookup and child processing still supply work and cancellation checks.

Preserve these exact identities:

- Occurrence keys remain `(arena identity, original node)`, distinct from semantic
  value identity. A known occurrence returns its completed identity, not an
  uncompleted container marker.
- Null/bool/kind tags, exact Decimal equality, UTF-16 units including unpaired
  values, ordered array child IDs and member-order-independent object key/value IDs
  are unchanged. Map/vector construction and its existing accounting remain.
- Interned equality still correlates the same predicate on equal values. Recursive
  consumption remains actual instance occurrence identity; equal child JSON must
  not be mistaken for a non-consuming recursive visit.
- On error, pending scratch drops. Previously completed child/value entries remain
  correct; an unfinished parent must never be published. A resource failure still
  aborts the appropriate attempt and clean later public evaluation uses a fresh Run.

Do not add a Run-wide pending Vec yet: its JsonRef lifetime/retention and error
cleanup would be a wider ownership change. The inline-first local Vec is sufficient
for this measured scalar allocation problem and avoids retaining borrowed input
occurrences between intern calls.

## Finite qualification and expected effects

`controls.json` freezes semantic and structural review groups. Preserve existing
exact-operand, semantic/correlation, recursive annotation, dynamic/recursive
reference, current-reference and interruption controls. New tests should exercise
scratch invariants directly in a copied/private test context and the corresponding
public original evaluator outcomes. They must not derive expected values from the
optimized engine.

Measure the exact original matched requests for reference-text-5000, unique-array,
unique-objects, large-literal, late-duplicate and equal-decimals with the reviewed
adapter and phase boundaries. Record the full output before counting a resource
improvement. Expected changes are fewer short-path backing allocations and no
one-element scalar pending-vector allocation; no claim is made that all value-key
retention, const comparison or composite allocations disappear. Retain the old
observations and use the same current toolchain/build mode for a local before/after
comparison. Do not regenerate input payloads or weaken the broader paired G6 gate.

Explicit low work/storage limits and cancellation still return their typed reasons
and permit clean retries. Exact numerical cutoffs may decrease because allocations
are actually removed; preserve the old cutoff history rather than forcing an old
Limit at a now affordable operation. Source/hash and actual native/minimum/host
qualification must follow the implementation; this design alone supplies no
performance or physical-memory PASS.
