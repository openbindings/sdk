# Borrowed equality and uniqueness for literal predicates

Proposed after checkpoint a7aab1e and the narrow evaluation scratch change.
The matched allocation pilot still shows 4.85 MiB incremental live data for a
20,000-number const comparison and 2.72 MiB for scalar uniqueness. These predicates
need an equality result, not the persistent semantic identities used to correlate
unknown effects. Keep Values::intern and its exact identity model for that latter
purpose. Do not modify effect subjects, occurrence identity, memo, or schema scope.

## Equality

Move the existing scalar comparison body into a private semantic value-predicate
module. Add one iterative borrowed comparison over original JsonRef values for
const, enum, and uniqueness collision verification. Numbers still use Run::number
and exact Decimal; strings still stream UTF-16 with cancellation checks between
1,024-unit chunks. Arrays compare corresponding elements in order; objects compare
counts and names independent of member order, using existing indexed scalar-name
lookup and exact UTF-16 comparison for unpaired names. A continuation frame holds
the two container views and the next child/member index; backend::member_at already
provides constant-time ordered member access. Retain only the ancestor frames,
not a copied key for every descendant or a pending list proportional to width.
All frame growth is charged by storage::reserve before publication.

Public evaluation already rejects duplicate-containing instances before this code.
A duplicate-containing literal cannot equal that admitted instance under the current
interner's sorted name/value-pair multiset model. Check that condition and return
false; do not silently interpret repeated names by first lookup. This is a proof
obligation, not a new duplicate policy. Schema/meta admission and duplicate-instance
NoVerdict remain unchanged. Private equality controls use duplicate-free operands;
public controls verify malformed/duplicate literal handling against current behavior.

## Uniqueness

Hash each array element over its borrowed value, then verify every repeated hash
by the same exact equality routine. Hash equality never establishes value equality.
Use a per-call RandomState. Numbers hash their canonical Decimal, strings hash their
exact UTF-16 sequence, arrays hash ordered child hashes with kind and length, and
objects hash kind, count, and a commutative sum of keyed (name hash, value hash) pairs.
The object accumulator is only a candidate filter; arbitrary collisions remain
sound because comparisons are exact. No hash is persisted or exposed.

One explicit continuation stack is reused between element fingerprints. Each frame
holds its borrowed container, next index, hasher/accumulator, and current member-name
hash. Scalar roots need no stack allocation. Parents consume completed child hashes
immediately; no full subtree hash table or canonical value copy is retained. Growth
and copied scalar scratch follow the existing Graph work/allocation controls.
Hash strings in bounded chunks. Charge each descent/member/combination before work.

Store (fingerprint, original array index) in one prepaid vector, sort by this pair,
and compare each candidate within a repeated-hash group against earlier members.
This is O(n log n) indexing plus exact collision comparisons, with O(n) requested
storage and O(depth) traversal scratch. Reserve the known element count once. Prepay
a conservative comparison-sort work bound with checked arithmetic before sorting;
check control before and after the sort. If measured timing makes sorting material,
consider a keyed index separately; do not add both implementations speculatively.
No external callback or fallible operation belongs in the sorting comparator.

Continue reading other primitive assertions as today. Hash the complete array
before sorting, then stop collision verification on the first exactly equal pair.
That pair is a definitive uniqueness failure; checking all equal pairs would add
quadratic work to an all-equal array without adding evidence. Explicit refusal
cutoffs can improve when unnecessary work is removed; historical quota cutoffs
are not compatibility requirements. Controls must preserve sound observations,
typed cancellation/failure and clean retry.

## Validation

Before implementation, independent design review must confirm the duplicate-literal
argument, hash invariants, cleanup, and prepaid growth/sort bounds. After implementation:

- Whole evaluator suite, exact operands, semantic correlation/cycles and interruption.
- Independent scalar/array/object equality controls: reordered names, escapes,
  unpaired UTF-16, adjacent wide integers, huge exponents, mixed kinds and late mismatch.
- Deep and wide values on the small-stack release control; no Rust recursion.
- Forced identical fingerprint controls for unequal and equal values, including
  different object shapes and repeated names in schema literals.
- Exact low work/storage refusal and cancellation throughout traversal, hashing,
  sorting boundaries and collision verification; clean retry; no retained owners.
- Original unchanged benchmark requests and complete observations for large-literal,
  unique-array, unique-objects, late-duplicate, mixed-equality and equal-decimals.
- Native, minimum compiler and refreshed Wasm/Node/browser qualification before
  declaring this final candidate qualified. Full paired G6 remains separate.

No performance tradeoff is accepted by this proposal. The large original interner
remains available only where identities have semantic purpose. The new private
module is an implementation mechanism for JSON equality/uniqueItems, with no new
public concept, API, limit setting or language-dependent result.

## Reviewed lookup refinement

Use a small internal-json `backend::member_candidates` bridge to the existing
immutable arena member index. It returns borrowed members from the exact-name-hash
bucket, or all members of an object below the existing indexing threshold. It
makes no equality claim. Prepay original name-token hashing and index search,
check cancellation between bounded UTF-16 hashing chunks, and use the identical
per-arena KeyHasher/write_u16 sequence used during parsing. Exact candidate-name
verification stays in the evaluator's chunked string comparison. No new object
index, decoded name allocation, or parser hash convention is introduced. The
candidate iterator owns no arena and retains only the immutable borrowed slices.
The same one RandomState must serve all element fingerprints in a uniqueness call.

Review verified that public duplicate-containing supplied literal resources already
refuse admission in ExternalSchemas::new. Preserve that existing boundary. The
private helper's duplicate-literal false result is defensive reasoning under its
recursively duplicate-free-instance precondition, not a new public verdict.

## Repeated-attempt control correction

The full evaluator kit exposed unstable cancellation counts when RandomState was
created inside each unique call: duplicate groups of different shapes can sort
in different orders and need different exact comparison work. The final design
owns one immutable keyed seed in the prepared Plan and borrows it for all outcome
and explanation attempts. No element fingerprint is retained. This preserves
keyed hashing, immutable/thread-safe plans, and repeatable candidate ordering and
checkpoint counts for one prepared contract. A public regression repeats both
detail modes and replays the exact uninterrupted control allowance. Independent
review reproduced the old variability and must verify the corrected ownership.
