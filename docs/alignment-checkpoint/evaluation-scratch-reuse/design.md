# Reuse call-local traversal scratch

Status: proposal for independent review before implementation. It addresses measured
allocation overhead without changing original predicate/effect identities or adding
a second evaluation path.

The matched pilot at df06588 reports 10,007 allocations / 1,441,908 requested bytes
for reference-text-5000, versus zero in the baseline's timed evaluation. Each alias
walk currently constructs and drops two backing vectors even when successive walks
have the same short shape. The vectors describe unfinished traversal, not schema
semantics or a reusable proof.

Keep one empty alias-path scratch owner in Run, never in Plan. Move it into one
alias traversal and restore it on every returned success/refusal. All aliases run
synchronously; alias resolution does not recursively call itself. Pop the path in
reverse insertion order after use so retained bucket capacity does not require a
full-width fill for every short later walk. Prepay cleanup work as entries are
inserted; cleanup after cancellation must release entries without requesting new
allowance. Hash/equality/context handling, cycle canonicalization and source
identities remain unchanged. Dropping Run frees the backing vectors. Panics drop
the local owner and unwind the call; no prepared state can retain it.

The unique-array job reports 20,045 allocations; Values::intern allocates a
one-element pending vector for each scalar, even for an occurrence already known.
Start traversal with one inline optional current item and an empty Vec. Only a
container needs pending backing. Reserve its actual vector growth through the
existing storage helper before pushing. Preserve the exact Decimal/UTF-16/array/
object canonical keys and occurrence memo, so semantic correlation and uniqueness
meaning do not change. This is a bounded first optimization; remaining identity
retention and composite literal costs still require separate measurement/design.

Validation: preserve all native semantic/effect/alias/uniqueness controls; explicit
low work/memory and cancellation must still refuse cleanly and retry successfully.
Exercise long-then-short alias paths, non-consuming cycles, dynamic scopes and
interrupted traversal without stale membership. Measure the original reference,
unique-array and unique-object requests using the same reviewed benchmark adapter;
confirm actual allocation reduction and stable outcomes. No new G6 residual is
accepted by this proposal. Full final comparison remains required.
