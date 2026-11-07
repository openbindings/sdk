# Borrowed literal values: independent implementation review

**Bounded PASS after the immutable Plan seed correction.** This approves the five-file borrowed equality/uniqueness implementation identified by `final-source-hashes.json`, over checkpoint `45447851d7cd7f93b4a6bf8d55a18b919df61070`. No SDK files were edited by this reviewer. This is not G6 performance acceptance or a whole-SDK qualification claim.

The 31 frozen design groups are mapped to actual evidence in `control-disposition.json`. On each of Rust 1.99 and minimum Rust 1.88, the final copied-source harness passes 14 independent evaluator groups, 16 existing custom-profile groups and two existing storage groups. Each toolchain also passes the independent forced-collision raw lookup group and both existing raw lookup groups. Commands, exact copied production prefixes, private bridges, logs and binary identities are retained here. The private evaluator build uses `opt-level=1` and the actual debug dependency artifacts; its depth-1,024 / 128 KiB native-thread control is evidence for that build, not a portable stack promise.

## Closed reproducibility finding

The parent full-kit run found that a new RandomState on every uniqueness call changes duplicate-group ordering and therefore work/checkpoint totals. Independent replay on one Plan with twelve differently nested duplicate groups reproduced twelve distinct Outcome totals (499–524) and 27–28 Explain totals across 64 repetitions. These were all semantic failures, but a budget/control calibrated from one run could interrupt a later run early. The pre-correction source, binaries and observations remain in `before-plan-seed/` and `*-before-seed-repeated-counts.log`.

The correction owns one RandomState in immutable Plan and lends it to every uniqueness call. It preserves exact collision verification, introduces no mutable shared evaluation cache and makes hash order stable within one prepared contract. The final independent test repeats both detail modes 64 times, requires identical full observations and one checkpoint total, accepts that exact stop count repeatedly, and cuts at every earlier checkpoint. Before the first verdict, interruption yields Cancelled NoVerdict. After failure is established, Explain interruption retains F with empty, incomplete diagnostics. Four concurrent callers also repeat the real uniqueness workload. Both toolchains pass. Independent prepares can have different salts; this review makes no cross-contract hash/order promise.

The design originally required one seed for an entire uniqueness call. This receipt strengthens that condition to immutable prepared-owner scope for repeatable evaluation of the same contract; it does not rewrite the earlier evidence as having covered that issue.

## Semantic and ownership audit

Exact equality uses borrowed current operands and ancestor continuations. Arrays use indexed positions; objects reuse the arena's existing name index and verify every candidate with exact decoded UTF-16 comparison. No cross-arena numeric node identity shortcut or scalar Unicode replacement is introduced. Exact Decimal equality/hashing retains the existing inline/wide parsing policy. Controls include alternative escapes, unpaired units, astral names, huge written exponents, adjacent integers beyond 2^53, reordered objects and a 20,000-element const with a late mismatch.

The duplicate-bearing literal defense is subtree-local. With the admitted duplicate-free instance, a duplicate-name literal cannot be equal; the helper checks that before first-member lookup can hide a duplicate. Public duplicate external const/enum resources still refuse registration, and root/deep duplicate instances still yield NoVerdict for const, enum and uniqueItems. Empty enum admitted by a permissive custom meta yields F for all six JSON kinds, while standard admission remains unchanged. Existing actual-meta exact-value tests pass.

Fingerprinting is only a candidate filter. One seed, kind tags, ordered array child hashes, canonical numbers and paired commutative object-member hashes preserve equal-implies-equal-hash. Every earlier candidate in a collision group is checked exactly; forced nonadjacent duplicate and all-unequal groups pass. Whole-array hashing precedes verification, which stops at the first exact duplicate. The 20,000-equal-value control verifies that early exit without a quadratic equal-pair walk. Forced unequal collisions exhaust work with typed Limit rather than an invented duplicate.

Pure const/enum/uniqueness runs leave Values empty. Undefined effect creation still interns values; equal-subject correlation, unequal-subject separation and consuming recursive arrays retain their outcomes. Frames and candidate vectors are call-local. On fingerprint failure, a partially filled ancestor vector is immediately dropped by its caller; there is no subsequent reuse of that failed owner. Successful calls empty the reused vector. After a deep first item, 1,000 scalar hashes request zero allocations.

## Lookup, resource and cancellation audit

The new member-candidate bridge borrows only the target object. It uses the target arena's unchanged KeyHasher and exact write_u16 stream. Periodic checks add no hash delimiter or seed. A callback failure cannot publish a partial hash. Small objects return all members; indexed objects return the matching hash bucket. The callback, query name and temporary iterators are not retained. Independent controls exercise 0/1/15/16/17/33/100 members, forced collisions, escaped-equivalent keys, missing keys, astral/unpaired names and every observed long-key checkpoint.

Ancestor and `(hash,index)` backing growth uses the existing actual-type-sized storage reservation before allocation. The independent suite checks exact/one-below work and storage, arithmetic extent refusal before allocation, every observed equality/hash/sort/verification interruption, and actual owner release. Representative equality/uniqueness/forced-verification paths expose 42/45/31 checkpoints; indexed long-name hashing exposes 67. A two-million-unit escaped string additionally exercises long streaming equality and midstream cancellation. Every interrupted owner returns to its counting-allocator baseline and a healthy retry succeeds.

The sort comparator is infallible fixed tuple ordering, has no callbacks and does not allocate. Its checked n log n charge is explicitly an SDK admission policy, not a universal exact std operation certificate. Sorted, reverse, alternating, vacuous and equal-fingerprint shapes are covered. No broader dependency-ledger claim is added by this review.

## Allocation observations and limits of this receipt

Six unchanged matched requests were independently replayed against the preserved release allocation artifact `adacc23a73cdd4302e789526d0c7d0a82f9bd0b775eda7445122c9bc4096e259`. Its embedded source/instrument provenance and request objects are retained in `allocation-replay.json`. Full observations match the frozen expectations and all six return to baseline live bytes:

| Workload | Outcome | Allocation requests | Requested bytes |
| --- | --- | ---: | ---: |
| large-literal | S | 4 | 488 |
| unique-array | S | 4 | 320,328 |
| unique-objects | S | 5 | 16,776 |
| late-duplicate | F | 5 | 320,377 |
| mixed-equality | S | 4 | 488 |
| equal-decimals | F | 5 | 425 |

That preserved release artifact precedes the Plan-seed correction. The final native source and cleanup are tested separately above; these observations are not silently relabeled as a final-seed build. Final followup: `final-seed-allocation-replay.json` independently repeats all six exact requests against the final-seed release artifact `77540fd3e68b818a039a45cbdd39ff126f0c0c517248208a77b58f9654f5ee83`. All full observations, allocation counts, requested bytes and incremental peaks equal the earlier rows, with no retained growth; embedded provenance and before/after binary hashes are checked. No timing conclusion is drawn from these samples, and this reviewer did not rerun Wasm for this slice. Parent full native/kit/host gates remain separate evidence.

Two harness errors are preserved separately: an incorrectly quoted empty JSON-string test and a temporary JsonValue whose borrow outlived the statement in supplemental Rust code. Neither changed a semantic oracle or SDK file. A stale binary invocation after the latter compile failure is retained and excluded from final supplemental evidence. Final rebuilt controls all pass.
