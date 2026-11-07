# Prepared numeric and count readings: independent implementation review

**PASS for the bounded optional cache at the four source hashes in [source-hashes.json](source-hashes.json), against SDK `4998c72`.** No production changes were made by this reviewer. All four files remained unchanged throughout final verification. The new untracked `operands.rs` is included in the source hashes and full copied-source inventory; ordinary `git diff` alone does not include it.

The implementation meets the accepted 256-byte token, 65,536-byte aggregate source, and 1,024-node-record policy. No correctness or accounting blocker remains in this slice. This does not qualify the whole ordinary engine, full once-prepared operand coverage, local syntax replacement, iterative traversal, original diagnostic witnesses, compiler allocation accounting, or matched performance completion.

## Code assessment

`operands::Preparation` chooses a reading from the original token's kind and byte length before optional parsing charges. Oversized or excess tokens remain `Original`; an absent node record invokes the original predicate path. The byte accumulator cannot exceed 65,536: each admitted increment is tested against the remaining total first. The only insertion caller visits each original node once, so the record count cannot pass 1,024. When that cap is reached, no record construction or token parsing occurs for further nodes.

The record holds only the active, present fields of one original node: at most five numbers, six sizes, and two contains counts. Counts are selected first, but `primitives` preserves the original numeric-then-size evaluation order and the array path retains the contains evaluation site. An inactive profile field is never selected through `Interpreted::get`. A count without active `contains` is not prepared or evaluated as a contains assertion. The containing Plan owns the unchanged source inventory; node indices do not become process-wide or URI-only cache identities.

Wrong-kind payloads become constant-sized `Invalid` markers without traversing or copying their JSON. Invalid positive-divisor/count requirements remain local readings rather than preparation failures. `Reading<Option<u64>>` preserves the distinction needed by the existing demand sites: a nonnumeric operand still fails through the primitive shape boundary, while a numeric noninteger/negative contains bound retains the contains-count refusal. These do not acquire invented Undefined atoms. Standard meta admission remains separate from the permissive-custom-meta controls.

The map record and exactly sized vectors are reserved before construction. The map reservation uses `(size_of::<(usize, Prepared)>() + 64) × 8`, plus the actual bounded Number/Size vector capacities and fixed scratch allowance. It follows the existing conservative insertion-only map schedule; keys are integer node indices and no authored name is copied. Parsing retains the previous `8L+1` work and `16L+256` allocation allowance. The parser never receives an unchecked numeric string: tokens come from exact JSON's `number_text`. Real reservation failure propagates immediately; there is no catch-and-defer path or fresh inner allowance. The new fields retain no preparation control.

`numeric_operand` borrows `&Bound` from a parsed reading. It neither clones the Bound nor copies its exponent. Schema-dependent runtime work/allocation is still charged before comparison, followed by the unchanged instance parsing and multipleOf scratch/domain policy. `Original` readings parse through that same charged path. Parsed counts instead return the copied `u64` result and remove their schema parse/conversion cost. Saturation remains the exact comparison policy for host-representable cardinalities.

No reference, effect identity, active-cycle, annotation, or complete-state memo machinery was changed. Counts enter the existing correlated `Matches` logic. The mandatory forced-original corpus and recursive/annotation coverage therefore remain relevant, rather than relying on an unchanged outgoing compiled fast path to hide a regression.

## Independent outcome evidence

The copied evaluator exposes only a private review bridge that calls the real `Plan::new` and returns a semantic-only DefaultValidator. No production logic is altered and no anonymous-root/dialect substitution is used. Source copies, the bridge, dependency hashes, build commands, and test-support fixture identity are retained here.

- **1,546 / 1,546 required cases passed through the original Plan**, with zero preparation refusals, configuration failures, or judgment failures.
- **All 59 pre-existing independent numeric design controls passed**, in both Outcome and Explain, with their original exact token strings and expected answers unchanged.
- **All three exact frozen container jobs passed at defaults:** 20,000 integer array S; 20,001-element late-invalid array F; 50,000-property integer object S.
- **35 additional cases passed:** token thresholds 255/256/257, aggregate and record-cap boundaries, unchanged large unused operands, orphan giant counts, all five simultaneous numeric predicates, wide/wide and negative-wide comparisons, invalid-divisor cache/fallback agreement, and 1,025 nodes carrying bounded wrong-kind markers under permissive custom admission.
- **Ten comparison allocation lanes passed**, including low-memory refusal and funded cached/fallback readings.

The frozen array and object still request 780 bytes with a 556-byte incremental evaluation peak. The late-invalid row requests 813 bytes and peaks at 589 bytes; its 8-byte reduction from the previous compact-engine receipt is the separately approved shorter generic failure message, not an accounting or oracle change. No timing/performance-baseline conclusion is drawn from these allocator controls.

Results are in [summary.json](summary.json), [controls-summary.json](controls-summary.json), [boundary-summary.json](boundary-summary.json), and the full `semantic-required-suite.json`.

## Allocation, limits, ownership, and cancellation

The focused public preparation probe uses a warm immutable environment so it measures the new per-preparation readings and ordinary admission rechecks without conflating them with already qualified cold environment construction. It covers 0, 1, 8, 64, 512, 1,024 and 1,025 operand-bearing nodes under eight allocation caps each: **56 lanes**, all with typed refusal or Ready as appropriate, bounded observed peaks, and no attempt retention after result disposal. Seven binary-searched exact allocation boundaries and their one-below refusals also passed. For 1,024 records the exact funded reservation was 4,384,584 bytes; 1,025 nodes required 4,385,800 because the final node correctly used fallback. The largest measured preparation peak was 545,402 bytes and requested allocation 731,576 bytes. These are workload observations supporting the source audit, not replacement universal coefficients.

Three exact work boundaries illustrate the intended timing:

| Warm preparation | First funded work allowance | One below | Default retry |
| --- | ---: | --- | --- |
| 256-byte prepared count | 2,070 | Limit | Ready |
| 257-byte original count fallback | 20 | Limit | Ready |
| Unused 3-million-digit minimum | 37 | Limit | Ready |

Thus an oversized operand is deferred before the source-sized reservation, not after poisoning the attempt. The same 256-byte count evaluates under 128 work units with **zero heap allocation**. The 257-byte fallback and a cached 202-byte wide numeric bound both honestly refuse that tiny evaluation allowance. Numeric caching has not erased the residual schema-dependent arithmetic cost.

For cached bounds with 100, 200 and 254 exponent digits, funded evaluation requests exactly `3N+3` arithmetic bytes plus the 33-byte failure message: 336, 636 and 798 bytes, respectively. Their incremental peaks are 202, 402 and 510 bytes. This matches the independent Bound comparison witness without an additional clone. A 255-digit exponent makes the full token exceed 256 bytes and correctly uses the more expensive original fallback. Every 512-byte program-budget comparison lane refuses instead of returning a semantic answer.

**96 preparation cancellation checkpoints and 83 evaluation checkpoints passed**, including clean retry. The 83 comprises 21 focused lifecycle checkpoints and 62 boundary-probe checkpoints. **60 concurrent prepared calls passed** across count, contains, and numeric-bound fixtures. The public control adapter also disposes document/proof/schema/resource/evaluator handles before reusing each prepared owner. No borrowed caller control or source handle is required to remain alive.

See `budget-results.jsonl`, `work-results.jsonl`, `comparison-allocation-results.jsonl`, and the retained Rust probe sources. These are current-toolchain macOS requested-heap measurements; they do not claim a new all-host allocation certificate. The implementer separately reports current/MSRV 270 tests, required 1,546 each, strict lint, Node 180, package/types, and both browser 505/1,546 gates passing.

## Harness issue preserved separately

The first test-support link failed with Rust E0460 because a reused local support artifact referenced a different rebuilt core artifact. The copied evaluator itself compiled. This was a review-driver artifact-selection failure, not an SDK semantic failure. `initial-driver-failure/` preserves the failed command/log. The fix compiles the unchanged current test-support source and exact fixture locally against the same selected dependencies, then links the consumer; no SDK source, fixture input, expected answer, or resource cap was changed. All later builds and replays succeeded.

**Closure:** the bounded prepared numeric/count cache is approved at the recorded source identities. Keep the raw fallback, complete per-node ownership reservations, retained comparison allowance, and unchanged default resource policies as invariants for subsequent engine work.
