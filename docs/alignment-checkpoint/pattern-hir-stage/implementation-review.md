# Independent inactive owned-HIR implementation review

**Bounded PASS.** The owned factory is approved as an inactive private stage. No production finding remains in this slice. This does not approve once-prepared VM activation, the final NFA construction/retry schedule, aggregate default jobs, or arbitrary-build physical-stack guarantees.

## Reviewed implementation

Reviewed actual `compiler/hir.rs`, its11 test groups and unchanged independent53-control file, the parent module's test-only activation, and the associated Reader hard ceiling. The factory fixes normalized depth33, AST representation depth250, prospective prefix64/equality256, and UTF8true before entering the pinned translator. Caller source limits have already been enforced by Reader. No second AST/form pair or externally supplied certificate is accepted.

Ownership and ordering meet the design: entry charge → guarded/prepaid AST → checked inventory → logical admission → checked HIR/source-error reservations → translator → destroy translator/diagnostic/AST → shared completion check → owned HIR. Failed logical admission cannot enter translation. Late cancellation cannot publish success. Local representation/profile refusals remain separate from shared sticky exhaustion and are never Undefined effects. The pinned error-closure proof remains valid; the implementation nevertheless prepays one defensive source copy and never formats or retains the backend diagnostic.

The final fixed work schedule is actual type-word based, **488 native units**, not the provisional64. `work-source-audit.md` records its source attribution and ownership coverage. It does not claim a physical-stack coefficient. All production hashes matched the tested copies at closure.

## Independent evidence

On **Rust1.99 and Rust1.88**, independent copied-actual-source probes each passed:

- **11,261 corpus/control rows:**11,253 successful HIR values equal to the independent pinned ordinary-parser baseline,2 required profile refusals,6 required normalized-grammar refusals. This contains the prior unchanged11,225 summaries/forms plus stage and resource controls. The separate design-stage Reader controls preserve earlier noncomparisons instead of counting them as HIR success.
- **3,217 resource/interruption rows:** exact and one-below work/allocation, zero caps, every observed cancellation checkpoint across ordinary/class/boundary and both rejecting profile families, clean retries, post-translation cancellation, all three prior causes, and nested outer/inner Attempt success/refusal. Ignored child refusal remained sticky in its parent.
- **12 targeted HIR-work precharge rows:** both direct recording controls and the real shared Attempt refuse before translator entry at the measured one-below boundary.
- **11 actual SDK unit groups**, independently rerun on both versions, including source/control owner release, immutable HIR reuse on concurrent threads, exact configured existence answers, all53 adopted controls and translation-entry instrumentation.

The full corpus requested **758,136,681 bytes** against **14,793,231,892 reserved bytes**, summed over independent attempts; every individual successful/refused row stayed within its reservation and every row ended with **zero retained tracked allocation**. All corpus and3,217 boundary observations match byte for byte across the two toolchains. These totals are not a single attempt or a default-budget claim. The native layouts were112/368/8/40/80/112 bytes for the six factory owners.

Parent actual Wasm evidence was inspected rather than independently rerun: exact pinned artifact221401d7460675b3e9cba40bd1b7b60bdd8e1af001ea0d832014f2658748cce5; Node22.23, Chromium156 and WebKit27.2 each11,305 rows,11,227 HIR successes,51 earlier Reader cases,8 grammar/profile refusals,19 independently predicted32-bit work overflows. Those19 remain refusals, never successful semantic comparisons. Parent records no undercharge/retention. This is concrete artifact evidence, not portability of native frame sizes.

## Controls, limitations and preserved failures

`control-disposition.json` maps all53 design controls to their actual probe/unit/source evidence. The synthetic D+1 arithmetic case is structurally unreachable in the fixed32→33 factory, but the checked operation is present; a public unlimited source request clamps to32, as separately tested. No test introduces a path for callers to raise prefix/equality limits.

The first independent recorder used Cell and failed the internal Control Sync bound. Replacing it with Mutex compiled, but macOS lazily allocated64 bytes per recorder lock, producing apparent192/256-byte retention during measurement. These were caller-owned test recorder allocations, released when the recorder dropped, not SDK leaks. Both failures and all initial observations are preserved; the qualified recorder uses allocation-free atomics. One Python harness syntax typo is also preserved. No semantic expected answer was changed because of those failures.

Remaining work is explicitly outside this approval: actual-HIR NFA inventory and logical compiler path, geometric successful/failed retry accounting, immutable retained programs/per-call cache integration, complete default100/300/400/1024 workload and performance evidence, and supported native/Wasm build stack qualification. The inactive stage and default Reader ceiling correction introduce no compiler activation or budget increase.
