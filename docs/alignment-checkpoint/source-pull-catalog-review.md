# Catalog relationship budget — independent review

**PASS for the bounded search-accounting correction and the explicit 64,000,000-work-unit policy decision.** The 10,000-item default job now completes in the independently copied production candidate. Allocation, entry and byte limits remain unchanged. This is neither a specification requirement nor an accepted timing regression. Whole provider execution and core edit/conformance allocation are outside this relationship ledger, as the API documents.

The initial default refusal is preserved in the parent's `/tmp/ob-source-pull-scale.{jsonl,log}` and the independent `pull-limits-original.jsonl`. All totals below cover one complete public `PullResult::validate_for`: request-byte accounting, response bytes, every relationship index/search, entry accounting, closure checks and edit metadata. Core edit and validation execute and must succeed, but their internal work/allocation is intentionally not included in these counters. The same request/validation attempt is used by `PullClient`; there is no reset at provider handoff.

| Catalog items | Original work charge | Corrected work charge | Reserved allocation bytes |
| --- | ---: | ---: | ---: |
| 0 | 264 | 264 | 112 |
| 100 | 502,868 | 238,472 | 483,232 |
| 1,000 | 7,818,260 | 3,484,664 | 4,839,232 |
| 10,000 | 113,927,204 | **46,765,436** | **48,471,232** |

The 10,000-row request document is 867,891 bytes; its compact request envelope is 867,924. Response is 1,833,449 bytes and conformant prospective document 2,163,463 bytes, with 10,000 operations/bindings and 30,000 indexed entries. The corrected charge still exceeds the old 20M default. Thus tighter accounting alone does **not** close scale. The root implementation decision explicitly supports this intended default consumer job at 64M. That leaves finite headroom, while retaining the 64 MiB allocation cap, 100,000-entry cap and 32 MiB handoff caps. It does not promise every possible 32 MiB carrier can complete within every other cap.

## Search bound and supported implementations

The actual Rust 1.99 compiler commit is `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`; MSRV 1.88 is `6b00bc3880198600130e1cf62b8f8a93494488cc`. Both define B=6, capacity 11 and non-root minimum 5 in their [1.99 node source](https://raw.githubusercontent.com/rust-lang/rust/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/alloc/src/collections/btree/node.rs) and [1.88 node source](https://raw.githubusercontent.com/rust-lang/rust/6b00bc3880198600130e1cf62b8f8a93494488cc/library/alloc/src/collections/btree/node.rs). The completed map invariant gives at least two root children and six children for each non-root internal node. See the [1.99 map invariants](https://raw.githubusercontent.com/rust-lang/rust/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/alloc/src/collections/btree/map.rs) and [1.88 map invariants](https://raw.githubusercontent.com/rust-lang/rust/6b00bc3880198600130e1cf62b8f8a93494488cc/library/alloc/src/collections/btree/map.rs).

Derived bound: a tree of edge height h contains at least `2·6^h−1` entries. Starting with minimum=1 and levels=1, advance minimum to `6·minimum+5` only while it remains at most the current entry count. Empty maps conservatively receive one level. At 10,000 entries this bounds five visited node levels, versus the previous fourteen. The [search implementation](https://raw.githubusercontent.com/rust-lang/rust/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/library/alloc/src/collections/btree/search.rs) follows one path and compares each key in a visited node at most once. Keeping 12 units per level and the existing query-byte cost safely overbounds its 11 comparisons, including tuple-section overhead already charged by callers. This is a source-derived correction, not a coefficient fitted to a timing result. Search happens on completed maps before insertion/rebalancing; these indexes have no deletion/reinsertion phase.

The checked recurrence's overflow means the next theoretical level requires more than usize::MAX entries, so stopping is safe. Charge multiplication and key_bytes+1 overflow must instead refuse. The production patch does so after checking cancellation/sticky refusal, and subsequent calls preserve the failure. Allocation reservations were not reduced by this reasoning.

## Independent verification and scope

`btree-bound.rs` ran **593,751** comparison controls on each supported native toolchain: ascending, descending and permuted insertions; present/absent lookups; sparse-tree transition counts 11/71/431/2,591/15,551 and their neighbors; zero and usize::MAX arithmetic. Both passed; maximum observed comparisons was 48. This supplements the source proof; observed comparisons alone are not its justification.

`public-threshold-probe.rs` reconstructs the unchanged catalog carrier bytes and determines the exact work and allocation thresholds separately by public calls. It also checks both exact thresholds simultaneously and each one-below refusal, plus exact/one-below request/response bytes and 3n entries. The production candidate independently passes default 10,000, has the same corrected thresholds, and passes its three unit groups including overflow/sticky failure. `production-candidate-thresholds.jsonl` and `production-candidate-unit-tests.log` retain the results. The first two-test unit artifact was stale after copying an older source mtime; it is retained as `stale-unit-artifact.log`, excluded from the result, and a touched private source rebuild produced all three expected tests.

No production file was edited by the reviewer. Full root tests, actual provider scale timings, Wasm and installed-package evidence remain separate gates. This review does not turn a resource refusal into successful throughput and does not weaken any relationship or semantic assertion.
