# Narrow integer predicate: bounded implementation PASS

The two pinned production files implement the approved change without a semantic or resource-policy finding. `value_predicates::integer` borrows the original Number token, performs the unchanged work/check call before parsing, returns true on successful i64 parsing, otherwise performs the unchanged reservation and exact Decimal classification. `typed` retains its Number short-circuit and all surrounding type-name/union behavior. No hint experiment, additional cache, record, owner, limit or default is included; `readings.rs` is byte-identical to committed mask baseline5cf.

`source-hashes.json` and `source/` pin the exact production delta: value_predicates `e1731c39927b297201afe4715ee6386aa670fff2ea5eb75f69cfd54a485a2b2a`, primitive `7d619cea1d032cf9df1ec87b18d1f4b680744de9a82999afe34290e82f3ca0a1`. The unchanged `number()` remains the old-path differential reference. No SDK files were edited by the reviewer.

## Results

Both Rust1.99 and Rust1.88 pass the bounded suite. The 52 frozen exact-token answers were established before implementation; none was changed from observed output. The direct test compares the unchanged number→is_integer path and new predicate on the same admitted JsonRef, then checks the independent answer and independently specified work/reservation formulas.

There are 520 funded/exact/one-below/zero-extra-storage rows per toolchain, including successfully pre-spent work/storage. Every observed direct control stop is replayed: another520 old/new comparisons. All1,040 comparisons per toolchain agree in result, exact ordered Graph work/storage and callback events, remaining allowances and retry outcome. The direct records also agree across toolchains. Narrow i64 tokens use zero reservation but still refuse insufficient work or precancellation; wide/fractional/exponent spellings keep the old reservation even when Decimal ultimately fits inline.

The inherited public comparison uses committed5cf mask observations, not pre-mask432. All106 complete public records remain identical, including callback totals, stop outcomes and original schema/instance locations. The36 separately instrumented public work/storage/control traces, including617 repeated interruption traces on the same owner/input, also agree exactly. Existing six exact-operand groups, the permissive empty-type control, the production numeric regression and the native frame backing check pass. Frame/Phase/State/Witness remain256/144/64/32 bytes.

Actual allocation probes run both old and new paths16 times for each token, after the value is admitted and outside trace-recording allocation. All52 paired rows match requested bytes, allocation counts and incremental peaks within each compiler build; all return to their original live-byte baseline. The16 successful-i64 inputs allocate zero on both paths. Five wide fallback rows differ between Rust builds identically on old and new paths; those existing build-dependent measurements are retained in `cross-toolchain-allocation-differences.json`, not hidden by a combined total or mistaken for a change caused by this patch.

## Frozen group disposition

| Group | Evidence |
| --- | --- |
| NI-01 | All52 exact tokens, including -0, i64 ends and adjacent overflow, 128-digit plain integer, 4,097-digit trailing-zero integer, decimal/exponent-equivalent integers, huge signed exponents and nonintegral fractions. Both old/new paths match the independent answers. |
| NI-02 | Wrong instance kinds remain F; integer/string unions remain S where appropriate. Null/number/unknown/nested/unpaired type names under permissive custom meta retain local unsupported behavior. Existing empty type union remains F. Standard admission code is unchanged. |
| NI-03 | Both orders of integer plus an invalid numeric type name still refuse under the permissive custom meta; the successful first member does not bypass the later error. |
| NI-04 | Exact/one-below work and storage, zero extra storage, and successful pre-spending compare full direct traces and remaining allowances. Formula expectations are independent of the old-path measurement. |
| NI-05 | Every observed direct and selected public cancellation boundary is compared, with healthy retry. Actual enclosing Attempt controls preserve ignored earlier work/storage exhaustion for narrow, fractional and wide inputs. Static inspection verifies that the same Graph calls preserve enclosing-control order; no separate exhaustive per-helper Preparation-Control cancellation schedule is claimed. |
| NI-06 | Decimal fallback is textually the existing parse/classification and charge policy. Full observations and exact operands retain original locations and later refusal behavior. No new integer approximation or branch skipping. |
| NI-07 |52 paired allocation/cleanup cases; zero allocations for all16 i64 inputs; no new ownership or retention. Cross-build fallback allocation differences explicitly retained. |
| NI-08 | Current/minimum compilation, selected complete public Outcome/Explain observations, frame accounting and exact operand tests. Unchanged broad large-capacity/auxiliary/host reruns are deliberately left to final qualification per the parent scope. Performance measurement is separate. |
| NI-09 | Existing JSON admission rejects +1,01,1.,NaN,Infinity and --1. No direct helper call is made on invalid tokens or non-Number values. |

## Driver corrections and limits

The first private builds lacked the trait import required to call Attempt charging methods. The next test counter used Cell despite WorkControl requiring Sync; it was changed to AtomicUsize and armed only after successful pre-spending. Both compile failures and the initial test source are retained. An initial postprocessing assertion incorrectly required equal allocator counts across compiler builds; the corrected comparison requires paired old/new equality within each build and retains the five observed cross-build differences. None required a production change or an oracle change.

The test copies instrument Graph charge/work and callbacks solely to observe order. They are not the uninstrumented performance binaries and do not supply timing evidence. This PASS approves the bounded implementation/source behavior; it does not recommend retaining an optimization whose later matched performance is unfavorable, accept a G6 residual, or qualify the discarded hint experiments.
