# Performance investigation result — inconclusive stop

The single preregistered diagnostic completed. Its method-adequacy guard failed,
so **no final qualification matrix was run and the authenticated slice remains
performance blocked**. Product runtime, all performance/resource budgets and both
historical failed results are unchanged. No observations were discarded or rerun.

Preregistration/instrument commit: `4437176`. Runtime checkpoint: `3315307`.
The [plan](performance-investigation-plan.md) was committed before timing, including
the fixed matrix, aggregation, component-wise resolution guard and stop decision.
The [summary](performance-investigation/summary.json) and
[provenance](performance-investigation/provenance.json) retain all outcomes.

## Result

Three fresh processes completed all four configurations for all four AND/64 KiB
rows: 48 configuration observations, seven samples each, 65,248 raw timing blocks.
The process outputs are preserved losslessly as `process-*.stdout.json.gz` alongside
build logs and environment observations. Each raw sample's order, minimum duration,
call counts and weighted arithmetic passed validation.

| Prospective check | Observed result |
| --- | --- |
| Balanced null rows satisfying all three 1 µs guards | **3/12** |
| Individual balanced null quantities within 1 µs | 22/36; **14 fail** |
| Largest absolute direct/adapter null delta, balanced | 1.899 µs |
| Largest absolute four-median null residual, balanced | 1.770 µs |
| Largest absolute four-median null residual, contiguous | 7.604 µs |
| Balanced real-auth rows satisfying the original numerical formula | 12/12, diagnostic only |
| Smallest balanced real-auth budget slack | 0.442 µs, below observed null variation |
| Complete final qualification | **Not run: prohibited by the stop condition** |

The balanced sampler reduced observed across-process residual SD in every row:

| Edition/layer | Contiguous null residual SD | Balanced null residual SD |
| --- | ---: | ---: |
| 3.0.4 binding | 3.725 µs | 0.792 µs |
| 3.0.4 operation | 4.099 µs | 0.885 µs |
| 3.1.2 binding | 0.976 µs | 0.853 µs |
| 3.1.2 operation | 3.482 µs | 0.244 µs |

These SDs describe three process observations per row; they are not confidence
bounds. Improvement in a residual can also conceal opposing component errors.
Only the 3.0.4 operation null row satisfied all three guards in every process.
The other nine row/process combinations each failed at least one guard. Switching
to a different, less restrictive adoption rule after seeing the results would be
outcome-dependent selection and is not done.

## What the evidence establishes

The observed variation is large enough to matter when subtracting four large
call totals to assess a small auth increment. Shorter, balanced blocks with longer
accumulation improved residual stability in this experiment, but did not meet the
prospective resolution target. This is evidence about the measurement, not proof
of the cause of historical budget failures or proof that the runtime passes.

Source review found no obvious auth-only traversal of the large body and no
justified runtime optimization for this investigation. The auth-path work concerns
context, credential selection and origin grants. That is a code-review observation;
this experiment did not include the extra isolation workloads needed to establish
payload independence causally. No runtime work was removed or shifted outside timing.

The null conditions are independently constructed objects. Therefore the experiment
cannot separate temporal sampling variation from persistent instance-placement or
cache effects. Frequent 2 ms switching also changes the measured regime; absolute
medians should not be directly compared with historical contiguous-run medians.

Load observations were retained. Sandbox restrictions denied CPU/performance-level
`sysctl` reads, and `pmset` reported unavailable thermal/power data (despite exit 0).
Those observations remain explicitly unavailable. No thermal event, interfering
process or other external disturbance was established or used to excuse a result.

## Functional qualification and provenance

The refreshed extracted-package qualification passed Rust tests/lint, native consumer,
TypeScript, Node/Wasm, lifecycle checks and core dependency isolation. The prior
minimal-invocation isolation evidence remains applicable to unchanged product code.
The first local-loopback test attempt hit sandbox socket denial; its log is retained
alongside the subsequently permitted successful run. Neither run executed timing.

The [receipt comparison](qualification-investigation/input-comparison.json) proves
the only changed qualified input is `examples/auth_measure.rs`. Product code and
functional consumer cases are unchanged from the prior successful receipt. The
measurement source, diagnostic runner, prospective final runner, plan and receipt
hashes were captured before timing. The measured executable is retained locally at
`/tmp/ob-auth-performance-investigation-20261013/candidate`, with its SHA-256 in
provenance. An additional hash-verified local copy is retained under
`target/openapi-auth-performance-investigation/20261013/candidate` so it does not
depend on `/tmp` retention. Executable build artifacts are deliberately outside Git.

## Independent review and next decision

Independent [GPT-6 Astra xhigh](astra-investigation-results.md) and
[Claude Opus 5.5 Extra](claude-investigation-results.md) reviews both accept closing
the investigation as `inconclusive-stop`, with the authenticated slice still blocked.
Both independently reconstructed the results from raw blocks and verified provenance.
Their acceptance does not override the failed method-adequacy guard.

Claude additionally observed a positive binding-layer adapter-null offset in this
sample and offered post hoc carryover analysis. These are exploratory clues, not
a causal explanation or a basis for changing acceptance. Claude corrected its earlier
noise-only interpretation and first-launch hypothesis as unproven. The proposed
small-payload AND isolation row has no historical measurement in this experiment.
The coordinator keeps the existing budgets: the relevant next design choice is the
measurement method, not a waiver of the 64 KiB requirement.

This bounded investigation stops here. AJ20 remains unresolved, and neither a
completion claim nor capability expansion follows. The next useful decision is how
to obtain adequate measurement resolution with the existing budgets intact. A future
bounded design could distinguish same-instance sampling error from independent-
instance placement effects and use direct stage measurements to locate real overhead.
It must justify any measurement change before another qualification. No new experiment,
threshold change or repeat is authorized by this report itself.
