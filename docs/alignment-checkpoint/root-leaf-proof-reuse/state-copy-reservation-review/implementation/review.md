# Empty State-copy reservation refinement — bounded PASS

Reviewed the global `State::copied` refinement on the root-alias-leaf memo candidate over `f62f47757ba6682bea316492056428a665a7bea4`. No SDK files were edited. The root-memo performance/retention decision and final G6 acceptance remain separate.

The only production change from the previously reviewed root candidate is the empty-map base in `semantic/mod.rs`: it is zero when **both** annotation maps are empty, otherwise the existing 256-byte base and all per-item/property/name reservations remain unchanged. The initial work charge, each name-work charge, and final `graph.charge(bytes)` retain their order. In particular, `charge(0)` still executes and reaches the enclosing preparation control. There is no root-only exception, refund, catch-and-continue, new owner, default increase, or changed semantic oracle.

`State` consists of a formula handle, optional shared witness Arc, and two BTreeMaps. Both pinned supported native Rust standard-library implementations return a fresh empty map when cloning an empty map; this also covers maps populated and subsequently emptied. The inline handles and shared Arc need no new owned allocation. State/Frame storage is accounted separately. This justifies removing this specific reservation; it is not a new claim that the nonempty schedule measures all physical allocator requests.

## Independent execution

Fresh private copies compiled on Rust 1.99 and minimum Rust 1.88. The exact new `copied` function was transplanted into the previously reviewed root probe copy; other production code and original root instrumentation were preserved. A review-only exact old copy function supplies the comparator. The actual benchmark adapter was copied byte-for-byte. Global requested-allocation instrumentation is confined to the private crate. Both builds and every executed test succeeded on the first run. Commands, build logs, probe sources/hashes and production snapshots are retained here.

Each toolchain passed **35 tests**, including the nine frozen State-copy groups and affected root, original-source ownership, Explain, public interruption and shared-preparation-budget groups. The 106 complete public observations match the pre-refinement root candidate, and their callback counts also match in these replays. All 798 public stop/retry boundaries pass. Root controls additionally replay 794 evaluation stop boundaries, ten target-marking stops, dynamic-context separation, original/subtree IDs, equal child/property-name exclusion, cut-dependent/limited predicate nonpublication, selected custom-meta use, and recomputed work/storage boundaries.

- Exact byte/work rows: empty **0/1**, three-unit property **518/6**, item **384/2**, both **646/7**. Empty, lone-surrogate, astral and multiple names were included. Nonempty old/new charges are identical; exact and one-below work/storage controls pass.
- Four direct trace comparisons preserve work/charge/callback order. Only the empty reservation argument changes from 256 to zero. All **28 actual shared-control stop boundaries** agree, including the zero-byte allocation callback.
- **48 empty-copy allocator rows** per toolchain cover true/false/real symbolic formulas, legal false witnesses, fresh and previously populated/removed/cleared maps, and one/1,000 copies. Every row requests **zero allocations and zero bytes**, with no retained clone ownership. Witness Arcs share the same seed and release their references.
- **18 nonempty allocator pairs** per toolchain cover property/item/both maps at widths 1, 12 and 64, with/without witness. Before/after request counts, requested bytes and incremental peaks match exactly; all clones release their owned storage. Both toolchains agree on every measured request/peak result. These measurements establish the sampled copy behavior, not a universal allocator certificate.
- Actual enclosing/nested Attempts preserve prior work failure, reservation failure, ignored tighter-child failure, checked aggregate overflow and cancellation. Healthy zero remaining reservation permits an empty copy; spent reservation does not. Graph checked-add and work boundaries remain enforced. No stronger sticky guarantee is invented for a bare Graph after an intentionally ignored private error.
- Source/witness ownership, exact effect identities, fresh Explain passes, cumulative diagnostics allowance and retry cleanup remain covered by the reused public/private suites. Native layouts remain **State 64, Frame 256, Run 984, Fields 8 bytes**. The change introduces no representation-layout change; this slice does not claim a new Wasm or physical-stack qualification.

## Exact 64KiB witness

The full original request is preserved in `64k-request.json` (SHA-256 `74f69f0da037c075939cb4cc0d771816baeb9c037aaebe39a3547a2ccf31e9e6`). Schema, value, supplied resources, URI spellings, mode, iterations and `program_bytes: 65536` are unchanged. The two historical witness rows differ only in their expected-observation field: baseline requires S, pre-fix candidate records its observed memory refusal. This review uses the unchanged **baseline S expectation**.

Using the exact current benchmark adapter's supplied-root preparation and evaluation path, the corrected candidate returns **Satisfies on three repeated calls on each toolchain**. The saved clean-f62 S observation and pre-refinement root candidate `semantic-analysis-memory-limit` evidence remain untouched. The restored result comes from removing the empty-copy reservation, not increasing the limit or changing the input.

## Source identity and disposition

`semantic/mod.rs` SHA-256: `160cdb7dc2f39a613542b49bd87c037d8fa76e34411ebda71909c29db15f4473`.

The other three root experiment files remain byte-identical to the preceding review:

- `semantic/execution/mod.rs`: `60c64c3f2689ff30ce412a3f184795532b7682b042d929d909fbba672766d588`
- `semantic/preparation.rs`: `9fded76a459726beb736b8f36b6e5bded22b5d7b638fa7e8f5f9a452dfb5aeab`
- `semantic/readings.rs`: `1a2663e4a817422c165ebf1592a017dc2d3f56ee1dd1a54daf035505344bc0e9`

`summary.json` contains the counted results and affected runtime thresholds. `current/minimum-copy-traces.json` and `current/minimum-allocation-observations.json` retain exact observations. `source-hashes.json`, `probe-hashes.json`, the old/new function copies, and `production.patch` distinguish production from review instrumentation.

No open implementation finding remains in this bounded refinement. It resolves the preserved 64KiB resource regression. Root memo may now be assessed against the separately frozen timing/allocation and negative workloads; this receipt does not accept any residual regression or complete G6.
