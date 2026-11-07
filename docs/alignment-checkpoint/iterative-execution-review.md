# Independent explicit-frame implementation review

**PASS for the bounded execution/ownership/semantic conversion at the exact files in `final-source-hashes.json`.** Both identified narrow corrections are present and independently retested. This does **not** close physical-stack qualification, matched performance, long-reference preparation, full pattern VM-once, cross-phase syntax facts, diagnostic witnesses, default-engine switching or outgoing compiler retirement. No production code was changed by the reviewer.

The implementation is based on SDK `46186957c2a4494405b92d6e1c5efdd8245347ea` with the uncommitted frame files. Complete copied evaluator sources, bridge/dependency/build identities and results are retained here. The only test bridge selects the original Plan without rewriting original schemas, profiles, resource pools or required results. Earlier evidence is preserved under `before-alias-and-guard-corrections/`.

## Findings and resolution

**R-IF1, P2 accounting omission, resolved.** `Run::alias` created the owned `#recursive-scope` string without reserving its bytes. The preceding dynamic-name allowance normally did not execute for a 2019 recursive reference, and the new concrete Path layout reservation does not cover heap contents of its generic value. The old blanket path allowance had been removed. Final code charges source-length work and the sentinel bytes before `.into()`. This was a demonstrated missing allocation-site charge, not a claimed whole-budget peak escape. Four additional original 2019 recursive-reference cases (depth 4/64, S/F) pass; every observed cancellation boundary for the depth-4 cases is also checked.

**R-IF2, guard-layout issue, resolved for the monitoring design.** Marking the wrapper alone non-inlined would not prevent its inner scheduler body from being inlined into the anchor frame, omitting that body's fixed frame from sampled deltas. `evaluate_bounded` is now separately `#[inline(never)]`; the outer live marker remains live across that call. This supports the reviewed sampled guard. It is not a proof of a hard stack ceiling. The separate `../design/stack-policy/review.md` remains applicable.

The additional alias-to-certified-leaf fast path is sound: pure cycles are handled first, alias entry has already established exact scope, and `begin` executes the certified leaf directly without a Visit/frame/memo entry. Its certificate excludes recursion and annotation production and is already consumed by refinement's no-producer proof. It preserves original predicate/equal-value effect identity. Initial source reproduced a 50k alias-valued object memory refusal; the final fast path closes it without a limit increase. The before result is retained, not relabeled.

No remaining actionable semantic/ownership finding was established in this bounded review.

## Independent executed evidence

| Gate | Result |
|---|---|
| Entire required suite through direct original Plan | **1,546 / 1,546**, no preparation refusal, semantic or configuration failure; both outcome/explanation observations retained |
| Existing independent numeric + frozen container controls | **62 / 62**, unchanged inputs/results; includes 20k valid array, 20,001 late-invalid array and 50k integer object |
| Pre-implementation frame controls | **36 / 36**, including all four previously failing mixed depth-32/64 S/F cases |
| Additional chain/scope/alias controls | **17 / 17**; real alias and non-alias chains at 8/64/256/1,024, finite negation chains, low program caps, alias-valued arrays/object, and exact invalid-pattern correlation under distinct scopes |
| Additional original 2019 recursive-reference controls | **4 / 4**, deep consuming recursion and exact S/F |
| Cancellation over three control groups | **4,830** observed boundaries, each requires Cancelled; clean reuse afterward; pre-cancel and owner-retention checks also run in the public driver |
| Applicator/annotation regression differential | **770** generated inputs agree with the prior reviewed original engine in both modes; prior output is comparison evidence, not a new oracle |
| Real storage module | **4** test groups, including an independent 8,000-operation forced-collision LIFO/clear/reuse model and failed-growth nonpublication |
| Small native thread | **12** public original-engine calls: depth32/64/256 S/F on a requested 128 KiB thread; each same input with 2 KiB configured guard returns Limit |
| Concurrent owner reuse | Eight threads × 16 cancel-then-success pairs on one prepared owner; owner/value Arc counts return to one |

The private evaluator was compiled with Rust 1.99 at optimization level 1; the small-stack/concurrent consumer was compiled at level 3 against that exact library. This is not an independent whole-release-build stack certificate. Parent current/minimum-Rust/host lanes are additional evidence owned by the parent and are not represented as independently rerun here.

The small standalone storage harness supplies a minimal Graph ledger/control stub so it can import the actual `storage.rs` without altering production. Its scope is Path behavior and allocation-before-publication; whole-Graph behavior is exercised by the public original-engine probes. An initial private probe required an explicit key/value type annotation; its before source is retained and the corrected four groups pass. This was a driver compile error, not an SDK failure.

## Semantics and lifecycle inspected

There is one complete scheduler and one child-result slot. All previous recursive schema applications are expressed by references, logical, object and array phases; no recursive `schema`/`body` fallback remains. Applicator partial state moves between phases rather than being cloned wholesale. Non-object/non-array bypasses, draft-07 reference-object suppression, empty logical folds under permissive custom admission, conditional arm ordering, exact name-token evaluation, contains counts and profile-specific annotations agree with the reviewed meaning.

Parents remain active while suspended. Completion pops the exact active Visit, gates every coordinate by final validity, checks **all** coordinate formulas for cut independence, stages only completed coordinate facts and memoizes only certified complete States. `Attempt::Abort` is structurally separate from a NoVerdict. Error/abort clears active membership and abandons incomplete parent folds; it cannot publish a parent it never completed. Existing complete memo/facts/opaque summaries and the cumulative Graph persist across retries exactly as before, while new pending facts become visible only at the existing boundary. A resource failure/cancellation cannot trigger retry or become Undefined.

The same original schema under distinct effective scopes remains distinct; equal maps in distinct Arcs compare by content. Outermost dynamic bindings and ambiguous `None` entries survive. The new entry sharing optimization tests 2019 node-sensitive recursive-anchor activation before returning an unchanged Arc. Pure alias-cycle canonicalization retains the queried-name projection and original identity guards. The original instance occurrence remains the Visit coordinate; exact equal-value identity remains a separate effect mechanism.

The original indexed object accessor is used directly and preserves exact JSON name/value occurrences. No object-wide collection or repeated nth-element scan was introduced. Prepared owners retain original arenas; call-local scopes/frames/Path indexes/return states do not escape into a shared cache. The public driver explicitly drops originating document/proof/schema/resource/evaluator owners and reuses each prepared value afterward.

## Allocation and remaining performance boundary

The two-vector Path uses semantic equality after hash comparison, exact ordered suffixes and LIFO bucket-head restoration. Rehash rebuilds prior links and heads; clear resets heads. Stored integer indices survive vector relocation. Collision work is charged. Checked full new vector capacity, actual `size_of` layouts and `try_reserve_exact` precede publication; a failed growth leaves no inserted key. Capacity is reused within an execution attempt. The frame vector is recreated on a new attempt, which is an honestly charged allocation rather than a hidden budget reset.

Shared unchanged contexts eliminate actual copied names. Changed contexts still reserve their temporary ordered map, final vector/Arc and owned strings. Context hash/equality scans remain work. The sentinel correction accounts for the one newly uncovered owned-string site. These schedules remain requested-allocation accounting, not a bound on allocator metadata or total process memory.

Selected independent requested-allocation observations (ordinary outcome; result-message ownership distinguished from attempt retention):

* Direct 20k array / 50k object: 1,780 bytes requested, 1,492-byte incremental peak, zero retained attempt bytes on S.
* Alias 20k array: 5,761,780 requested, 1,492-byte incremental peak, zero retained attempt bytes on S.
* Alias 50k object: 14,401,780 requested, 1,492-byte incremental peak, zero retained attempt bytes on S. The many short-lived per-alias Path allocations remain a possible timing optimization; this result is not a timing PASS.
* The frozen deep workload is now S with 100,564 requested / 51,448 peak bytes. The former depth refusal is not a useful matched performance baseline for the newly completed job.
* 1,024-node alias and non-alias chains complete at defaults. Their 64 KiB program variants refuse honestly; actual requested allocations stay below that cap apart from the separately retained small terminal result where applicable.
* F/N observation counters may retain the returned message/code until the driver consumes the outcome; that is not a leaked Run or prepared mutation.

The sampled guard remains an additional refusal mechanism. It does not measure peaks wholly between checks or establish an engine-call-stack bound in Wasm. Existing symbolic recursion bounds remain, and the complete fixed scheduler/predicate/Graph/helper path and margin must still be qualified per supported host/build. No full hard-stack claim follows from the 12 small-thread calls.

## Exact open gates

Both frozen repeated-reference-text workloads (1,000 and 5,000 sibling spellings) **still refuse preparation-work-units**, as before. Their inputs/results are in `frozen-observations.jsonl`. The frame conversion was not represented as closing that distinct preparation problem. Reference fan-in and the real deep/long chains pass.

The added TypeScript/Node/browser controls genuinely invoke the public API with an explicitly supported custom profile, ensuring these finite cases use the original engine even before a default switch. They cover depth32/64/256 S/F and 50,001 alias items S/F in Outcome and Explain, with owner cleanup. Their original schema meanings are clear and no generic provider certification is asserted. I reviewed the fixture/wiring read-only; parent owns actual host execution evidence.

This bounded PASS approves the complete execution conversion and its corrected ownership/accounting changes. OE-05 repeated-spelling preparation, OE-06 full immutable pattern VM preparation, cross-phase syntax sharing, diagnostic witnesses, matched performance and ordinary compiler removal/default switching remain open. No required result, fixture pool, public limit or cap was weakened to obtain these results.
