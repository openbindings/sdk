# Independent call-local pattern cache review

**PASS for this bounded correction.** The implementation fixes the demonstrated epsilon-breadth reservation defect without moving VM construction into preparation. No remaining cache-specific blocker was found. The general VM parser/HIR/compiler construction certificate remains **open**; this report does not qualify its existing multipliers or full VM-once preparation.

Reviewed `semantic/patterns/cache.rs`, its integration in `patterns.rs`, the checked addition in `decision.rs`, and the actual locked regex-automata 0.4.18 allocation sites. Exact production/dependency hashes and copied-source equality are recorded in `source-hashes.json`. No production edits were made.

## Source-backed schedule

The four sparse-set backing vectors are initialized once from empty through `ActiveStates::new/reset` (`pikevm.rs:2011`). The two slot vectors use exactly `N * G + max(G, 2 * P)` elements (`SlotTable::reset`, line 2087), with the pinned Vec minimum capacity accounted for. `Option<NonMaxUsize>` is word-sized. Each per-vector byte layout is checked against isize::MAX before asking the backend to allocate, and aggregate arithmetic is checked.

`epsilon_closure_explore`, line 1664, records a state in the destination set **before** expanding it. One Union contributes at most arity−1 pending entries, BinaryUnion one, and Capture one restoration. Thus one initial Explore plus the sum of those counts bounds the maximum pending stack even when many branches point to the same compacted state. The stack drains between closures; searches clear its length without discarding capacity. The pinned amortized growth schedule gives cumulative requested capacity below four times max(pending, 4), with a three-word bound on the private frame layout on the supported targets. These facts justify the new term; the old state-count term could not.

The one-cache/one-engine, no-reset, no-clone invariant is explicit and holds at the current call site. A future clone, reset to another engine or different dependency/toolchain allocation strategy requires a new reservation/review. `Cache::memory_usage()` is not used as evidence for cumulative requests. The state scan is preceded by charged work/control checks, with another check before creation. The integration also charges its separate matching-cost state scan. The Graph checked addition prevents a maximum budget from accepting wrapped cumulative bytes. Errors propagate out of preparation of this call-local matcher.

## Independent execution

`cache-probe.rs` invokes the **production cache module and real Graph implementation** (a copied Graph appends only a read-only counter accessor). It does not duplicate the reservation formula. A recording allocator covers cache construction and repeated searches, including captures and a three-pattern NFA exercising the general G/P terms. Each requested total is below the production reservation and every cache owner releases all measured storage.

| Input | Reservation | Actual cumulative requests |
| --- | ---: | ---: |
| 100 empty alternatives | 9,920 | 1,792 |
| 1,000 empty alternatives | 96,320 | 16,192 |
| 10,000 empty alternatives | 960,320 | 160,192 |
| Nested repetitions/alternation | 37,024 | 12,160 |
| Capturing groups | 2,512 | 1,616 |
| Three patterns with captures | 6,608 | 5,328 |

The remaining four cases cover a tiny ordinary pattern, Unicode alternatives, classes and 500 real alternatives. Searches repeat on the same owner, with both is_match and actual capture searches in the G/P controls. Tests exercise the exact production charge and one byte below, work exact/one below, both cache control checkpoints, vector-layout and arithmetic overflow, prior Graph reservations and original matching semantics. **23 private tests pass**, including the five production cache controls and inherited parser/decision tests. The initial direct rustc test-module path error is retained separately as a probe-driver error; the isolated copy changes only the test-module path.

The final copied evaluator was independently forced through original Plan: **1,546/1,546 required cases pass**, with no configuration failures, plus **62 inherited controls in both modes**. These include the unchanged 20,000-item array, 20,001-item late-invalid and 50,000-property jobs, retaining S/F/S under original defaults. The copied production cache/integration/Graph bytes match the reviewed source. This is outcome evidence, not a new performance baseline or an Explain implementation qualification.

The coordinator separately reports 926 current workspace checks, 320 minimum-version library checks, strict lint, 184 Node tests, 352 installed-package assertions and both browsers' 505 core/1,546 required plus five cache controls. Those broader lanes are coordinator evidence; this review independently performed the native allocator/private tests and forced-original replay above. Their existence does not close the separate VM construction certificate.
