# Bounded independent implementation review

Result: **PASS for the two-file scratch-reuse increment over a7aab1e**. No production finding remains. This is not G6 acceptance, a timing qualification, or acceptance of remaining canonical-value retention. The SDK was not edited by this review.

`source-hashes.json` and `final-source-hashes.json` identify the exact unchanged production files. `production.patch` contains the reviewed delta: 32 insertions and 8 deletions in `semantic/mod.rs` and `semantic/values.rs`. The private copy preserves those files byte-for-byte before appended test modules. Its crate-level unsafe prohibition was removed only to install a System-forwarding counting allocator; no production evaluator behavior was instrumented or replaced.

## Ownership and accounting conclusions

`Run::alias` takes its own scratch, calls the unchanged walk, captures either result, reverse-pops every published entry, restores the now-empty scratch, and returns the captured result. No fallible cleanup step, callback, or early ordinary return skips restoration. One tick precedes each attempted path publication and pays for later cleanup. Failed push/growth leaves the existing entries and bucket links valid; reverse pops restore all bucket heads. Cleanup depends on live entries, not an earlier walk's retained bucket width. Returned contexts own their Arc independently. Active schema membership, memo/facts, scope identity, and cycle suffix selection are unchanged.

`Values::intern` processes the initial item inline through the same loop. Scalars and known occurrences allocate no pending-vector backing. Containers reserve actual replacement backing before publishing the expanded parent and each child; object pairs reserve both slots before either push. The former fictional 64-byte-per-pending-item allowances are removed, while canonical-key/map, string, number, and child-work charges remain. The initial and per-child cancellation checks remain. Parent identities are published only after every descendant has completed. Completed child identities may survive a refused intern call, but no unfinished parent does, and a clean retry preserves exact equality.

No new unchecked reachable vector growth was found. Saturating pending lengths cannot conceal an achievable usize overflow: nonzero tuple-size Vec storage would exceed its representable byte extent first, and `storage::reserve` checks the replacement extent before reserve/push.

## Independent controls and evidence

The 17 frozen design groups are mapped to executable evidence in `control-disposition.json`. Twelve private test functions pass on **both Rust 1.99 and Rust 1.88**. Final commands and logs are retained. They include:

- Repeated shared aliases; a 128-node walk followed by 1,000 short walks and the reverse ordering; 20,000 alias-to-leaf integer applications and a late invalid item.
- Static self/two-node cycles with independently deciding siblings; all six existing distinct dynamic-scope controls; explicit 2019 recursive-anchor inheritance, including the nested required-property failure.
- Every observed alias cancellation checkpoint: 283 long-chain, 33 static-cycle, and 28 dynamic-cycle boundaries. Every byte allowance below the successful 8,928/352/1,220 reservations also refuses without retained path membership. Work cuts exercise the same restoration invariant. Retrying the same Run scratch after replacing only its failed graph succeeds.
- 110 forced-collision push/growth boundaries, including failures after a backing allocation, followed by reverse-pop/refill and exact suffix/lookup checks.
- Exact decimal equivalence, adjacent wide integers, huge exponents, escaped/astral/lone UTF-16 units, object order versus array order, empty/nested structures, and composite operands from different JSON arenas.
- Equal values at distinct occurrences preserve correlated effects; unequal values remain independent. Descending through an equal child still constitutes consumption.
- All 53 observed interning cancellation checkpoints, including 39 after some children completed; all reservation cuts below 3,890 bytes; all work cuts below the exact 405-unit success. No refused parent is published; clean retries recover exact cross-arena equality.

Measured successful and interrupted owner lifetimes return requested live allocation to their baseline after drop on both toolchains. Warm short alias walks request **zero** allocations and consume no additional reservation after the long walk. Initial null/bool/small-integer interning makes exactly the two existing canonical/occurrence map allocations; a known scalar hit requests **zero** allocations and succeeds with zero new storage allowance, while zero work and already-cancelled controls still refuse.

The initial private harness failed to compile because it moved an internal value out of a shared reference. The driver was corrected to borrow it; initial diagnostics are preserved separately. No SDK fix or oracle change resulted.

## Unchanged workload replay

An independent replay used the exact parent final allocation binary, verified by SHA-256 `d3a52ca07322b25601481d18438c0d63a0c39d1bba0edf9e4fcaaf941b9accb2`. Request objects for the five pilot-overlap workloads match exactly; `equal-decimals` comes unchanged from the current frozen fixture generator. All six expected outcomes pass and each evaluation returns live bytes to baseline. `matched-workload-replay.json` retains complete observations and request hashes.

| Workload | Required | Allocations | Requested bytes |
|---|---:|---:|---:|
| reference-text-5000 | S | 7 | 1,908 |
| unique-array | S | 45 | 4,915,564 |
| unique-objects | S | 6,036 | 1,178,628 |
| large-literal | S | 88 | 10,158,120 |
| late-duplicate | F | 46 | 4,915,597 |
| equal-decimals | F | 7 | 685 |

These observations substantiate the intended scratch allocation reduction. They also preserve the substantial remaining value-key allocation/retention. No matched timing claim or residual performance waiver follows.
