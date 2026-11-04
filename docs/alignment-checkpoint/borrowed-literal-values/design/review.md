# Borrowed literal predicates: bounded independent design review

**Approve the proposed representation for implementation and qualification, with the conditions below.** No SDK implementation is approved by this design review. This is a focused replacement of needless literal-value interning, not another engine, a public equality API, or a G6 residual acceptance. The current scratch checkpoint is 4544785; exact design/source/authority identities are in `source-hashes.json`.

The pinned current core text (`inputs/current-c1419c9-5815fb1/spec/openbindings.md`, c1419c9) requires exact UTF-16 string/name and decimal-number equality. Array order and object member association retain the JSON Schema meanings already implemented. Its resource requirements do not require copying every literal descendant into a retained canonical-value map. Borrowed traversal is a coherent smaller mechanism for const, enum and uniqueness; effect identities and occurrence-based recursion must remain separate.

## Duplicate-literal argument and its actual public boundary

The argument is sound **with a recursively duplicate-free admitted instance**. At any corresponding object, the old interner's sorted pair vector preserves every member occurrence. A duplicate-name literal has repeated first coordinates; a unique-name instance has distinct first coordinates. These multisets cannot be equal, even when their lengths match or repeated values are equal. If the duplicate is nested, an array's corresponding child or an object's corresponding name/value child differs by induction. A kind or member-set mismatch already makes the containing value unequal.

This is not a general equality definition for duplicate-bearing JSON. Keep the helper private and its admitted-left precondition explicit. A cached duplicate check must apply to the literal **subtree**, not the entire owning arena. `JsonRef::to_owned().has_duplicate_names()` already queries that subtree without reparsing or copying; a small borrowed internal helper is also acceptable. Do not serialize either operand, or choose a repeated name's first value before checking the precondition.

Five actual baseline public probes—duplicate const, same-count duplicate object, escaped nested duplicate names, enum with a clean later choice, and an unrelated duplicate literal under `$defs`—all refuse in **ExternalSchemas::new**, with `resource repeats JSON member names`. `duplicate-baseline.log` preserves this evidence. Preserve that registration/admission boundary. The private false-result proof must not become a new public F classification. Duplicate instances retain the existing NoVerdict. A finite independent multiset model corroborates the argument in `duplicate-model.json`; the structural argument, not that finite enumeration, covers general trees.

## Minimal private representation

Equality needs one current pair and a Vec of ancestor continuations: container refs, next index, and any scalar resume state. Push only unfinished containers. Array `element` and `backend::member_at` are constant-time indexed access; do not restart iterators with `nth`, retain a child-width pending queue, or create temporary owned JSON keys. Frame growth is charged using its actual type size before publication. Exact kind/scalar/length mismatch can finish immediately; all remaining predicates keep their existing execution ordering. Local scratch drops on every Result, and no frame escapes into Plan or memo.

Fingerprinting needs a separate current borrowed value plus one reused ancestor stack and a completed u64 slot. Parents consume each child result immediately. Object frames retain only their accumulator, cursor and current name hash. Scalar roots require no frame allocation. Keep the random hash builder alive for the **entire uniqueness call**, using identical fresh builders from that state for equivalent recursive structures; do not reseed per element, frame, member, or retry within the same array.

The important soundness implication is `exact_equal(a,b) => fingerprint(a)==fingerprint(b)`. Kind and length tags, ordered array child hashes, canonical Decimal hashing, exact decoded UTF-16, and a commutative sum of **paired** name/value hashes satisfy this by induction. Summing names separately from values is unnecessary and worsens filtering. Hash collisions are permitted. Reordered object members, alternative escapes, negative zero, normalized decimal exponents and different arenas must not produce a false-negative fingerprint. Chunk boundaries must depend on decoded-unit counts, never raw source escape lengths. All fingerprints remain private and ephemeral.

## Candidate lookup bridge

The proposed `backend::member_candidates` bridge is preferable to a second equality-local object index. Reuse the arena's existing index: small objects yield their at-most-15 members; indexed objects yield the exact hash bucket. The caller verifies every candidate using bounded exact UTF-16 comparison. This also removes the current unmetered scalar `Run::named -> object.get` path from the new algorithm.

Required bridge conditions:

- Use the target arena's existing KeyHasher state and exactly its `write_u16` stream. Chunk checks must not insert hash delimiters, lengths, new seeds, or alter parser/index hashes. Equal decoded names must land in the same bucket.
- Precharge source extent/name hashing and bounded index lookup; check between bounded UTF-16 hashing chunks. Charge each candidate and its actual exact-name comparison, with chunk checks for long equal prefixes.
- Return borrowed candidates tied only to the object arena. Do not retain the work callback, control, query owner or a mutable arena borrow. Do not construct copied keys, a second index, or a candidate Vec.
- A hash hit is never a member-name match. Test forced collisions and missing names across the 15/16/17-member threshold, including escaped, astral and unpaired names. The existing index keeps only the first occurrence of a repeated decoded name; that is harmless only under the explicit duplicate-free comparison precondition.

## Uniqueness, sorting and interruption

One checked/prepaid `(fingerprint,index)` vector plus ancestor scratch is sufficient. Reserve the whole known record extent before filling it; bytes and growth use actual layouts. Zero/one-element arrays may return true immediately after validating the uniqueItems operand shape: no pair exists. That is optional, straightforward proof-based work elimination.

Within an equal-fingerprint group, compare each candidate against **all earlier members until an exact duplicate is found**. Checking adjacent sorted records alone is unsound when collisions interleave unequal values. After the whole-array hashing phase, the first exact duplicate establishes failure; stop verification there. All-equal arrays must not enumerate all equal pairs. Forced-collision all-distinct arrays remain bounded by configured Graph work and return typed Limit if it is exhausted, never a guessed duplicate or Undefined.

Use an allocation-free sort of the fixed-size records. `sort_unstable` is a reasonable first implementation. Prepay the checked `n log n` SDK sort-admission charge and check cancellation both before and after. **Describe this as a scoped SDK policy charge, not a portable exact dependency-operation certificate.** If the implementation instead claims a precise comparison upper bound, supply its source argument or use an explicit iterative heap sort with metered comparisons/swaps; do not introduce a guessed coefficient under that stronger label. No fallible callback or Graph mutation belongs inside the std comparator. Timing qualification decides whether this simple sort remains suitable; do not add a second hash index speculatively.

Preserve existing pre/post semantics for uncooperative exact-number parsing and hash operations, including size-proportional work and scratch before entry. Strings/names use bounded decoded UTF-16 chunks. An interrupted equality/hash/sort/verification cannot return a Boolean. Any new earlier definite false/true result may avoid unneeded work, but this slice must not short-circuit other primitive predicates or change effect/memo/annotation behavior.

## Frozen gates

`controls.json` freezes **31 independent groups** before implementation: exact scalar/composite equality and hash equivalence; duplicate boundary; candidate index thresholds/collisions; nonadjacent fingerprint collisions; all-equal early stop; deep/wide and long-then-short scratch; checked exact/one-below budgets; every observed cancellation phase; owner release; effect correlation and consumption; concurrency and custom-meta use.

Run the unchanged matched requests for large-literal, unique-array, unique-objects, late-duplicate, mixed-equality and equal-decimals. Keep full outcomes and original diagnostics, actual driver/source identity, current/minimum Rust and actual Wasm evidence. Measure the proposed allocation reduction and any timing cost. Existing full evaluator/exact/effect/recursive/interruption and small-stack gates remain. No threshold increase or failed-workload reclassification is approved here.
