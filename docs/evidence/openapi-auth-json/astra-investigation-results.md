# Independent Astra review — bounded performance investigation results

Date: 2026-10-13. Reviewer: GPT-6 Astra, xhigh, independent subagent.
Preregistration commit: `443717671a90e1863876b50b000862bba3834d39`.
Product runtime checkpoint: `33153076564f0997c6a7e364660d16ae7aa0eece`.

Disposition: **accept `inconclusive-stop`; do not proceed to final performance
qualification under this investigation**. Only 3 of 12 balanced null rows meet all
three preregistered 1,000 ns resolution guards. The fixed investigation is complete;
the authenticated JSON slice remains performance-blocked. Both prior failed CPU
runs remain failures. The balanced arm's 12 numerical auth passes do not override
the failed method-adequacy condition or qualify the slice.

I independently recomputed the delivered evidence before reading any Claude result
review. I ran no builds, benchmarks, qualification executions or measurement reruns.
The only work performed was reading existing files, calculating from retained raw
observations, checking hashes/Git differences and writing this report. Old binding
drafts and derivative authorities were excluded. Experimental implementation
behavior is not treated as binding-specification authority.

## Raw evidence and arithmetic audit

The three compressed process outputs contain all 48 planned configurations:
four edition/layer rows times four configurations times three fresh processes.
Every process reports success. Configuration rotation is exactly
`(row_index + process_index) % 4`; across the 12 row/process combinations, each
configuration occupies each position three times. There are 1,344 condition samples
and 65,248 raw timing blocks: 672 contiguous and 64,576 balanced blocks.

I independently checked every block's sample, cycle, order, position, condition,
positive integer call count and minimum duration. Contiguous condition orders
alternate forward/reverse and retain seven samples of at least 50 ms. Balanced
samples follow the prescribed Williams orders, finish complete 16-block cycles and
stop only after every condition has accumulated at least 200 ms. The preceding
complete cycle had not already satisfied that stopping condition. No block or
sample is omitted from aggregation.

The smallest retained contiguous sample is 50,000,209 ns. Balanced condition samples
range from 200,005,750 to 233,655,957 ns; individual blocks range from 2,000,000 to
33,400,417 ns. The long block is retained. The protocol specifies minimum durations
and whole calls, not a 2 ms upper bound. Its cause is not established by this record.

For every sample I recomputed `sum(elapsed_ns) / sum(calls)`, then the seven-sample
condition medians. All recorded samples, medians, direct/adapter deltas, residuals,
formula ceilings, slacks and numerical verdicts agree. The diagnostic uses the
original formula `Ca - C0 <= max(0, Da - D0) + 5000 + 2*S`, with real-auth encoded
context sizes 334 and 332 bytes. It applies no null correction and does not replace
differences of medians with medians of differences. All across-process component
and residual SD fields independently recompute correctly.

The sampler preserves condition-position balance. The narrower claim about
predecessors is also accurate: each other condition is a predecessor once within
the four-condition orders, while concatenated order/cycle boundaries add further
transitions. The final preregistration discloses this limitation. This was not an
unplanned schedule change discovered after timing.

## Control decision and interpretation

Each balanced null row must satisfy all of `abs(Da-D0)`, `abs(Ca-C0)` and
`abs((Ca-C0)-(Da-D0)) <= 1,000 ns`. Recomputing that rule gives:

| Edition/layer | Passing null rows | Largest absolute null quantity across three processes | Residual SD, contiguous | Residual SD, balanced |
| --- | ---: | ---: | ---: | ---: |
| 3.0.4 binding | 0/3 | 1,898.711 ns | 3,725.072 ns | 791.536 ns |
| 3.0.4 operation | 3/3 | 928.914 ns | 4,098.886 ns | 885.406 ns |
| 3.1.2 binding | 0/3 | 1,621.625 ns | 975.589 ns | 852.552 ns |
| 3.1.2 operation | 0/3 | 1,605.053 ns | 3,481.831 ns | 243.878 ns |

Across all balanced null rows, the maximum absolute direct delta is 1,605.053 ns,
adapter delta is 1,898.711 ns, and residual is 1,770.370 ns. Component guards matter:
3.1.2 binding/process 1 has a residual of only -64.333 ns, but its direct and adapter
deltas are 1,096.952 ns and 1,032.619 ns. Their cancellation cannot establish
adequate measurement resolution.

The balanced residual SD is smaller than the contiguous residual SD in all four
rows, but these are SDs of only three process observations, not confidence bounds.
Improvement in this diagnostic does not satisfy the failed absolute guard. The
preregistered decision correctly yields `inconclusive-stop`; the runner's specified
return code for that decision is 2, consistent with the coordinator's reported exit.
All planned observations were completed and retained despite early guard failures.
The coordinator confirms that no final performance qualification was run.

The balanced real-auth arm has 12/12 numerical formula passes; the contiguous arm
has 10/12. Both are diagnostic. The smallest balanced auth slack is **441.560 ns**,
for 3.1.2 binding/process 1. This is below the observed balanced null variation,
including the same row's across-process null envelope. It cannot establish adequate
resolution near the acceptance boundary or show that historical failures were noise.
The historical formulas, every-pair obligation and 15% baseline-spread rule are
unchanged. This candidate-only diagnostic is not the full candidate/closed matrix.

Null conditions were independently constructed anonymous instances. Therefore these
observations combine sampling variation with possible instance-placement effects;
they do not distinguish those causes. Absolute results from the frequent-switch
regime also should not be compared directly with historical contiguous-sampler
medians as evidence of a product optimization. Product runtime did not change.

## Provenance and scope

I verified the following SHA-256 values against the actual files. Every source,
runner, plan and receipt listed below also matches its blob in committed
preregistration `4437176`. The commit timestamp precedes the first recorded process.
All 83 `testedInputs` in the qualification receipt match the current workspace.

| Artifact | Verified SHA-256 |
| --- | --- |
| Measurement source | `265c04b1c3c3f775cce492588181f324c05d4bc61fa956e590ba4e09c8b58677` |
| Diagnostic runner | `2c6da3377deaf479c06eb989e43c96524098d815ae9233748a9c10899d5c9069` |
| Final qualification runner | `99b0341c5c0750b8ec9731ff0d21535fb6dde2420a6d4f40f80113f696f4aea6` |
| Preregistration plan | `08962a1459de211a12999c7b78ab523ba89d652f7fab38ddcea217cffc3e1e4e` |
| Qualification receipt | `ccd8d47b8d0e40a9ae7f941289c531e70b63d209c0aef5860a4a18fcdd5c0347` |
| Measured binary | `711571feff841e4816d3e90ecafddad23ce19272a6c111f42a94453c9171fae5` |
| Diagnostic summary | `b8fe373d8c43671e170e94eab7b631a2ade368cbe7c998fca486991cac879df7` |

The executable is retained locally at
`/tmp/ob-auth-performance-investigation-20261013/candidate`; it was deliberately
excluded from the Git evidence copy. I hashed that retained executable directly.
The final pre-timing runner additionally pins receipt and plan hashes before allowing
balanced qualification. Its added integrity checks do not alter the performance
formulas. The plan excludes legacy-mode qualification for this investigation and
requires normal Python execution so raw-integrity assertions remain enabled.

Git comparisons confirm that product crates, the frozen performance policy and both
historical CPU result directories are unchanged from runtime checkpoint `3315307`.
The reviewed runtime still exposes no concrete auth-only traversal of the 64 KiB
payload that would justify an immediate product repair. That is a bounded source-
review finding, not proof that no optimization exists anywhere in the implementation.

Environment observations were attempted and retained. `sysctl` returned permission
errors; the thermal command returned error text despite exit status zero. CPU/performance
level and thermal observations are therefore unavailable. Load averages and timestamps
are recorded, but they do not establish the cause of the null differences or historical
failures. No external-disturbance exception is justified by this evidence.

## Grounded next step

Honor the stop now. Preserve the failed historical gates and this inconclusive
diagnostic; do not increase the 1,000 ns guard, tune blocks, discard configurations,
correct auth measurements with null estimates or invoke final qualification.

If Matthew later authorizes another investigation, first review a new method design
that separates same-instance relabeling from independently constructed instances,
with fixed crossed instance/label ordering and identical complete invocation work.
The present null controls cannot make that distinction, and their component biases
are large enough to matter at the observed margins. That causal control question is
a better-supported next proposal than another unchanged timing run or speculative
runtime edits. Any future profile-led optimization should identify actual redundant
work while preserving grant/refusal behavior and would still need the original full
qualification. No such further experiment or optimization is authorized by this
report or started as part of this review.
