# Bounded performance investigation — preregistration

Frozen before diagnostic timing on 2026-10-13. Matthew authorized this investigation
with the existing budgets intact and independent Astra and Claude review. Runtime
checkpoint: `33153076564f0997c6a7e364660d16ae7aa0eece`. Both prior CPU runs remain
failed. The slice remains blocked until a complete, valid qualification passes.

## Question and scope

Can a better-resolved measurement distinguish the adapter's incremental auth work
from variation in four approximately 350 microsecond total call costs? No external
disturbance has been established for the old runs. A method experiment cannot prove
that their failures were noise or regrade them. No runtime change is justified by
the current source review; product code remains unchanged for this investigation.

Only the consumer timing harness, measurement tools and evidence may change. Core,
roles, public interfaces, independent client pin, binding specifications and website
remain outside scope. Old binding drafts and derivative expectations are excluded.

## Fixed diagnostic experiment

One experiment, three fresh candidate processes, four rows: AND-authenticated POST
with 64 KiB request/response, OpenAPI 3.0.4 and 3.1.2, binding and operation layers.
Each row runs four configurations with independently constructed function objects:

1. Contiguous 50 ms sampling, real auth.
2. Contiguous 50 ms sampling, null controls.
3. Balanced 2 ms blocks, real auth.
4. Balanced 2 ms blocks, null controls.

Rotate this configuration order left by `(row_index + process_index) % 4`, where
rows are 3.0 binding, 3.0 operation, 3.1 binding, 3.1 operation and processes are
0/1/2. Thus each configuration occupies each position three times across the 12
row/process combinations. All conditions are rebuilt for each configuration.

The four conditions are D0, Da, C0, Ca. Null conditions use identical anonymous
source/context work in both labels, constructed independently, including source
caches and host state. This tests combined sampling and instance-placement variation;
there is no same-object-only null variant in this bounded experiment.

Both methods perform the same separate one-call oracle/preflight, which checks exact
wire values and one host effect and warms the source. There are no extra untimed
calls before timing blocks. This standardizes preflight in the diagnostic; the
original default harness retains its historical per-sample warm-up. Other call work
is identical to the registered harness, including input conversion, owned attempt
credentials, request preparation, origin/header checks, finite response conversion,
destruction and operation completion. No body work moves outside the timed closure.

Both samplers retain seven samples. The contiguous arm alternates forward/reverse
condition order and times each condition for at least 50 ms. The balanced arm uses
Williams orders `[0,1,3,2]`, `[1,2,0,3]`, `[2,3,1,0]`, `[3,0,2,1]`, always completes
a full 16-block cycle, and accumulates **at least 200 ms per condition per sample**.
Blocks last at least 2 ms and contain whole calls. Each sample is total elapsed
nanoseconds divided by total calls for that condition, not an average of block means.
Raw block duration, call count, sample, cycle, order and position are preserved.
Predecessor balance holds within the four-condition orders; concatenating orders
and cycles adds boundary transitions, which remain visible in the raw schedule.

The 200 ms accumulation is fixed prospectively and satisfies the frozen >=50 ms
minimum. No sample or process is discarded. A fixed 10-second post-build idle period
runs once before measurement; there is no outcome/load-dependent warmup or waiting.
Record CPU/performance levels, load average, thermal report and timestamps before
and after each process. Missing environment observations remain explicit.

## Method decision, independent of real-auth pass/fail

For every balanced null row in every process, require all three quantities:

- `abs(Da - D0) <= 1,000 ns`
- `abs(Ca - C0) <= 1,000 ns`
- `abs((Ca - C0) - (Da - D0)) <= 1,000 ns`

All terms use **differences of the seven condition medians**. These are additional
engineering resolution checks, not statistical confidence bounds, product budgets,
or evidence resolving historical sub-microsecond failures. Report component biases,
residuals and across-process standard deviations for both methods, even if they
cancel in the combined quantity. Never subtract null estimates from auth results.

**If any balanced null check fails, stop as inconclusive.** Do not adjust thresholds,
add samples, tune block duration or run a qualification hoping for a passing outcome.
Every completed process and raw observation remains part of the result. Finish the
fixed diagnostic matrix even if an early row fails, to avoid selective reporting.

If all checks pass, permit exactly one complete qualification using the balanced
method for every row and both binaries: same 24 unsecured and 16 auth gates, three
fresh alternating process pairs, matched measurement source and closed SDK snapshot,
seven >=200 ms condition samples. The closed binary uses four independently built
anonymous conditions in balanced mode to match the candidate's condition schedule.
The formulas in `performance.md`, exact encoded context S, each-pair requirement,
15% baseline-spread rule and all resource budgets remain unchanged. No paired-
difference substitution, null correction, cheaper baseline or dropped case is allowed.
The final runner is frozen and hashed before diagnostic timing. It requires an
adequate diagnostic record via `--balanced-qualified-by`, selects `balanced` for
both binaries, uses the same fixed 10-second post-build idle period and validates
raw orders/durations/sample arithmetic. The runner also checks unchanged candidate
qualification and preregistration hashes. Legacy reproduction remains available
without the flag; only a final summary with `sampler: balanced-200ms` is eligible
for this investigation. Execute Python normally, without `-O`, so raw-integrity
assertions remain enabled. A failure or inconclusive final matrix
leaves the slice blocked. Auth slack smaller
than observed null variation must be reported honestly, not treated as proof that
historical failures were measurement errors.

## Independent rationale and bounds

GPT-6 Astra xhigh independently supports the counterbalanced design, component-wise
1 microsecond null guard, fixed 200 ms accumulation and unchanged runtime. It warns
that differences of separately selected medians are sensitive to drift and that
15% process spread is not an uncertainty bound on those differences.

Claude Opus 5.5 Extra independently recommends a null-control comparison with more
balanced sampling and increased accumulation, and finds no justified runtime repair.
Its full opinion is retained in `claude-investigation-design.md`. Its suggested
alternative uses fixed-count mirrored blocks, more diagnostic workloads and an SD
adoption rule. This experiment adopts the smaller Williams matrix and Astra's stricter
per-process/component resolution target instead; this is a coordinator design choice,
not a claim that Claude prescribed the exact protocol. Claude's claim that failures
fit instrument resolution is a hypothesis, not an established cause. Its phrase
"null SD at least allowance / 5" appears inverted; no decision relies on that phrase.

Bound: one diagnostic experiment; at most one measurement-harness repair (the
prospective sampler here); at most one final matrix only after method adequacy.
No second runtime optimization or unchanged rerun. Both reviewers examine the actual
implementation and results before the investigation is closed. Slice completion is
conditional on all original qualification obligations being satisfied. An inconclusive
stop completes this investigation, **not** the authenticated slice.

Absolute medians from the balanced frequent-switch regime are not directly
comparable to historical contiguous-sampler medians. Null controls cannot separate
sampling variation from independent-instance placement effects. No extra isolation
workloads or same-object controls are included; neither causal distinction is claimed.
