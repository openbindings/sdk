# Measurement resolution — synthesized execution proposal

Date: 2026-10-13. Design only. Matthew requested independent coordinator/Claude designs
and their synthesis; he has not authorized execution of this new proposal. No runtime,
measurement code, published interface, binding specification or performance result
changes in this design pass. The authenticated slice remains performance blocked.

The two proposals and comparison are retained alongside this plan. This is the best
supported combined design from the current evidence, not a claim of a proven optimal
measurement technique. Claude independently reviewed the synthesis and endorsed its structure, then found
a sampling-effort feasibility issue. The final design incorporates that correction
and the reconciliation recorded in comparison.md. An implementation review may
discover a flaw before timing. Future independent instrument/results reviewers are
GPT-6 Astra xhigh and Claude Opus 5.5 Extra; the coordinator integrates their findings.
The current design comparison used the coordinator and Claude only.

## Fixed authority and intended result

Preserve every budget/formula and existing result in ../performance.md. In particular:

- all 24 unsecured and 16 incremental-auth gates, both editions and both invocation layers;
- candidate unsecured cost<=1.10*closed+1000 ns, closed SDK abc9602;
- Ca-C0<=max(0,Da-D0)+5000 ns + 2*S ns, exact encoded context S;
- differences of seven condition medians; each of three fresh process pairs must pass;
- >15% baseline process spread remains inconclusive;
- complete request/body conversion, owned attempt credentials, preparation,
  origin/header checks, host work, result conversion/destruction and completion;
- every resource budget, source-cache boundary and independent client pin unchanged.

No null subtraction, output/body shortcut, small-payload substitution, changed median
statistic, failure removal, replay-until-green or performance waiver. Old OpenAPI
binding drafts/derived expectations remain excluded. This is measurement work only.

Answer whether a single fixed method has adequate resolution, separating same-object
timing from provider-instance and immutable-fixture-construction effects. A successful
control study permits one full qualification. It does not qualify the slice itself.

## The candidate method

### Object identity and fixture ownership

Implement one fixture/condition factory with explicit IDs and a separate mapping
from measurement labels to retained objects. A same-object comparison must borrow
the same object from the same index, not two equivalent closures. Resolve the index
before entering the timer. No RefCell/locking/dispatch wrapper is introduced only
for one path or one label.

The proposed qualification method shares caller-owned immutable JSON input storage
and the finite response-byte buffer among its compared conditions. The direct and
adapter code still performs the exact current per-call conversions and copies.
No prepared request, per-call parsed response, owned credential or invocation result
is shared. Keep separate anonymous/authenticated source state and separate provider
caches, each permanently associated with its own source. Never alternate auth and
anonymous sources through the same one-source cache.

For resolution controls, construct two data regimes, each with direct instances DA,
DB and adapter instances CA,CB. In the common-data regime they share the same retained
fixture input/response storage. In the separate-data regime each instance receives
independently allocated, byte-identical fixture storage, as in the old harness.
All contexts are anonymous and semantically identical. Both regimes remain alive
through their mapping comparisons. Factory assertions verify intended shared versus
distinct fixture identity and intended alias versus distinct condition identity.

For each row/process construct two cohorts:

- F construction order: commonDA,commonDB,commonCA,commonCB,separateDA,separateDB,
  separateCA,separateCB;
- R construction order: the exact reverse of that list.

A/B are stable IDs for physical instances. Build independent cohorts and keep them
alive through all seven samples and all mappings. Reversing creation order is a
controlled contrast, not a claim that every memory-placement factor is controlled.
Choose cohort creation order F/R when (process+row)%2==0 and R/F otherwise; do not
reconstruct objects between measurement labels or samples.

Run wire/effect preflight once per physical object, not once per alias: exact input,
body, target and headers; one host effect; successful response conversion; completed
invocation and output destruction. Verify warm-source behavior with the maintained
source-preparation checks. Timed code remains free of exploratory instrumentation.

### Fixed blocks and cycle

Every timed block contains exactly K full calls:

- K=64 when request or response payload is 64 KiB;
- K=256 for other original workloads.

These values are chosen before results. 64 calls amortize switching over substantially
more calls than the old approximately 6-call blocks; 256 keeps small-call blocks long
enough to amortize timer overhead. There is no calibration or outcome-dependent K.
Every call in a timed block, including the first after switching, stays timed.

Use cyclic slot sequence S=0010203112132233. It contains every one of the 16 ordered
predecessor pairs exactly once, including self pairs and the wraparound. For each
sample, rotate S left by
`(sample + 5*row + 3*process + 7*configuration + 11*cohort) % 16`
and reverse that sequence when `(sample+process+cohort+configuration)%2==1`.
Zero-based cohort indices are F=0, R=1. Configuration indices are the canonical IDs
below, regardless of their actual execution order.

Execute one untimed K-call block of the resulting sequence's final slot before the
first timed cycle of each sample. This gives the first timed block its declared
cyclic predecessor. There is **no** untimed lead-in before subsequent timed blocks.
Complete whole 16-block cycles until each label accumulates the fixed target:

- at least **800 ms** for the original AND 64 KiB workload, all its R controls and
  all its Q conditions/sentinels, including the closed anonymous conditions;
- at least **200 ms** for every other original workload.

Select this from the frozen workload ID and size, not measured speed or whether
that process is anonymous-only. Keep identical targets for candidate and closed. Seven samples per label; each sample=sum(ns)/sum(calls). Preserve the complete
chronology and aggregate by semantic label, then take its seven-sample median.

Reserve 16,384 raw block records per configuration/cohort before timing;
serialization/file writes occur after the row. The explicit per-sample cap is
146 complete cycles, so seven samples use at most 7*146*16=16,352 timed records
plus seven untimed predecessor-prefix records: 16,359 total. Raw records explicitly
tag prefix versus timed blocks; prefixes never enter sample duration/count sums. If any
condition has not met its duration target by that limit, retain all data and stop
incomplete. No hidden reallocation, dropped block or shortened duration.

This is a work/memory cap, not a claim that K and duration alone imply a maximum
record count. At an illustrative 350 µs/call, an 800 ms large-condition sample needs
ceil(800ms/(4*64*350µs))=9 cycles, or 1,008 records over seven samples. At 8–12 µs/call,
a 200 ms small-condition sample needs 17–25 cycles, or 1,904–2,800 records. These historical-
scale estimates justify headroom; actual faster/slower execution cannot change the
cap or required duration. No RSS/heap grade follows from this instrumentation limit.

## Phase R: controls only, one fixed experiment

Three fresh candidate processes; four AND 64 KiB rows in order:
0=3.0.4 binding,1=3.0.4 operation,2=3.1.2 binding,3=3.1.2 operation.
Within each process/row, both F and R cohorts run these four configurations:

| ID | Configuration | D0,Da identities | C0,Ca identities |
| --- | --- | --- | --- |
| R-alias (0) | Same-object/common-data | commonDA,commonDA | commonCA,commonCA |
| R-common-normal (1) | Distinct/common-data, normal | commonDA,commonDB | commonCA,commonCB |
| R-common-swapped (2) | Distinct/common-data, swapped | commonDB,commonDA | commonCB,commonCA |
| R-separate-normal (3) | Distinct/separate-data, normal | separateDA,separateDB | separateCA,separateCB |

This is **96 configurations**, each with seven samples. It contains no real-auth timing
and no new 1 KiB AND workload. It evaluates one proposed method; the separate-data cells
are causal controls, not another candidate method that can be selected after results.

For each sample 0..6, visit cohorts F/R or R/F according to (sample+process+row)%2.
Within each cohort rotate canonical configuration list [0,1,2,3] left by
`(sample+2*process+row+cohort)%4`, then reverse if (sample+process+cohort)%2==1.
Retain this complete schedule in the preregistration. It distributes order exposure
without claiming perfect factorial balance across seven samples. Each sample still
contains every configuration/cohort exactly once. No process-level pooling hides
which retained object pair or mapping gave a result.

### Adequacy

For every configuration 0/1/2 in every cohort,row and process, calculate from medians:
D=Da-D0, C=Ca-C0, R=C-D. Each must satisfy the original engineering resolution target:
`abs(D)<=1000 ns`, `abs(C)<=1000 ns`, `abs(R)<=1000 ns`.
That is 72 eligible configuration observations and 216 component checks. All must pass.
The 24 separate-data observations remain fully reported and cannot be used to qualify
the shared-data method or to rescue a failed eligible observation.

Every sample/order/identity/duration/fixture/provenance validation must also pass.
Any eligible control failure means **inconclusive-stop**. Finish the fixed matrix
unless a corruption, effect failure, user interruption or hard cap prevents it.
No alternative rule, pooled signed mean or null correction replaces a failed check.

Report D,C,R for every mapping and construction cohort. Same-object failure identifies
inadequacy before independent-instance effects. Distinct/common failure with same-
object success exposes remaining instance sensitivity. Shared versus separate data
and retained-instance label reversals provide controlled associations; they do not
prove cache placement, allocator behavior or a thermal cause. Do not claim the old
failures are explained away.

Define U_R(row)=maximum(abs(D)+abs(C))over its eligible R observations. This preserves
components that can cancel in R. It is an empirical ambiguity screen, not a confidence
interval, error bound or adjustment applied to auth numbers.

## Phase Q: one conditional complete qualification

Only after both reviewers accept adequate Phase R may the already-frozen Q runner
start. No code, fixture, plan, sampler or receipt change is allowed between phases.
If R is inconclusive, Q must refuse to launch. The following Q schedule and checks
must be implemented/reviewed before R timing, not designed after a favorable result.

Use the identical shared-fixture factory, full-call K blocks, cyclic schedule and
sample arithmetic for candidate and closed SDKs. In each process, retain objects
through samples. Closed processes use four anonymous condition objects, as before,
under the same schedule. Run three fresh pairs, closed/candidate for pairs0/2 and
candidate/closed for pair1, with the original 24 workload rows and all 40 gate calculations.

Q preserves the current 24-row ordering: edition 3.0.4 then 3.1.2; within each,
workloads bearer-get,key-post,and-post,large-token,anonymous-get-large,
anonymous-post-large; within each, binding then operation. R rows 0..3 map to Q
rows 4,5,16,17. Freeze exact fixture hashes, not just this naming convention.

For each of the 16 authenticated candidate workload rows, retain a dedicated anonymous
common-data null pair on each path alongside the real conditions. First create one
common fixture, then create eight objects in F order:
`realD0,realDa,realC0,realCa,nullDA,nullDB,nullCA,nullCB`.
Use its exact reverse for R. Choose F if (pair+row)%2==0, R otherwise. These are four
real objects total plus four null objects total, not eight per path. All share only
the declared immutable input/response fixture. Every source/provider state remains
dedicated. Retain these objects through all samples and mappings. Preflight each
once. For the closed binary and the eight non-auth candidate rows, use four objects
in F order D0,Da,C0,Ca or the reverse by the same rule. Closed Da/Ca execute anonymous
work, preserving the four-slot shape. Candidate auth rows include two null configurations
per sample round; closed rows have Q-real alone. This macro-schedule asymmetry is
explicit: block/cycle/sample algorithms match, but the complete process timelines
do not. No claim is made that it has zero effect. The original unsecured regression
and baseline-spread requirements still apply without correction or an exception.

Use three namespaced Q configurations: `Q-real`(ordinal 0), `Q-null-normal`(1),
`Q-null-swapped`(2). The null maps are the same normal/swapped within-path pair maps
as R-common. For each sample round, rotate this three-configuration list left by
`(sample+pair+row)%3`, then reverse if(sample+pair+row)%2==1. Thus controls and real
work use the same sample-round interleaving policy as Phase R and remain close in
time, rather than running seven-sample sentinels only before and after a real row.

Both seven-sample null observations must pass all three 1 µs component/residual checks.
They verify instrument adequacy in the qualifying process, not just in Phase R.
Use process=pair and cohort=0 for F or 1 for R in the fixed cycle rule. Configurations
have separate string namespaces across R and Q; the ordinal is only for deterministic
ordering. Validators reject wrong-phase IDs. Closed/non-auth-only rows have one
`Q-real` configuration with the same cycling/sampling logic and no auth-null gate.
The K value and duration target still match candidate/closed for each workload.
At the 800 ms AND /200 ms other targets, the minimum measured call time is 2,150.4 seconds
(35.84 minutes) for R, and also 2,150.4 seconds for Q across all six processes. Whole-cycle
overshoot, prefixes, preflights and persistence add wall time; roughly 40 minutes is a
planning estimate, with 45 minutes as the hard per-phase cap.

The new anonymous-only factory/alias/sampler must compile and pass its preflight
against the exact closed SDK abc9602, which has no auth support; do not use candidate-
only APIs in that mode. This is an explicit readiness requirement. Standalone legacy
reproduction uses a separate mode and cannot produce a qualifying Q receipt.

Calculate every original formula, baseline spread, absolute median and ratio unchanged.
For each authenticated row define U_Q(pair,row)=maximum(abs(D)+abs(C))from its two
null observations. For64 KiB auth rows use U=max(U_R,U_Q); for other original auth rows
use their matching U_Q. Compare the numerical gate's slack to U without subtracting
U or a signed null estimate from any measured timing.

- Any original numerical gate failure remains a recorded failure.
- Baseline spread>15%, failed validity controls, corrupt/incomplete observations, or
  a numerical auth pass with slack<=U prevent completion as inconclusive.
- Only all 40 gates passing in every pair, valid controls, and auth slacks>U permit
  performance qualification, followed by independent result verification.

Keep numeric outcomes and validity outcomes separately: an invalid instrument does
not erase an observed numerical failure, and a numerical pass cannot bypass invalid
controls. No acknowledgement/waiver path turns a resolution-limited result into a pass.
The envelope is conservative observed-data evidence, not a statistical guarantee.
Existing non-performance slice obligations still apply before overall completion.

## Verification, provenance and environment

Before timing:

1. Implement only the measurement factory/sampler, two runners, deterministic schedule,
   raw validator and appropriate harness tests. Keep product/runtime APIs unchanged.
2. Test identity aliasing/sharing, exact cyclic edge counts including wraparound,
   construction/mapping schedules, all sample totals, known-value medians/gates,
   exclusion of untimed-prefix records from sample arithmetic,
   and rejection of deliberately corrupted raw records. These are deterministic
   correctness tests, not exploratory performance runs. Validators use explicit errors
   that remain active with Python -O; do not rely on assert for acceptance decisions.
3. Obtain independent instrument review. Allow one bounded repair round before any
   measurement. Unresolved defects stop readiness; do not run to learn if code works.
4. Build sequentially against qualified extracted packages and the exact closed
   snapshot. Either use a fresh archive receipt or an independently reviewed harness-
   only extension proving all runtime/functional inputs identical and compiling the
   new harness against those packages. Preserve the old receipt and name the actual
   checks; never claim an unperformed full functional rerun.
5. Commit the final plan, full expanded schedules, exact fixture/mode table, source/
   runner/validator/receipt/compiler/target hashes and status-classification code.
   Both runners reject mismatched hashes, wrong sampler IDs and any incomplete record.

Each copied binary gets one retained preflight-only first launch and then a fixed
10-second settle before its phase's timing starts. Preflight includes both editions/
layers and checks expected wire/effects; no timing estimates or calibration emerge.
Log CPU/platform/performance-level/thermal observations when available, load and
start/end timestamps before/after each process. Missing observations stay missing.
Capture permitted metadata before timing; no policy changes, CPU pinning, stopping
unrelated applications, load-conditional waits or unproven disturbance exceptions.
No builds/tests/reviewer commands run concurrently with timed processes.

Retain compressed raw blocks with actual object IDs, fixture IDs, label mappings,
constructor ranks, predecessor/order, complete-call counts and durations, including
all failures and partial output. Keep source and recipe in Git, binaries in a named
local non-Git directory, and their hashes in provenance. Persist a validated raw
checkpoint after each row. A controlled abort also flushes partial-row blocks. An uncatchable
crash may leave the current row incomplete; it must not discard earlier saved rows
or produce a passing result.

Reviewers independently recompute the outputs and inspect identity/work equivalence,
not just accept runner summaries. Preserve old failed runs and the inconclusive study.
Never equate completion of this investigation with completion of the auth slice.

## Work limit and stop

This document is the design deliverable; no execution now. If later authorized:

- one measurement-only implementation change set and one pre-timing repair round;
- exactly one Phase R matrix: 3 processes, 4 rows, 2 cohorts, 4 configurations, 7 samples;
- at most one Phase Q matrix after R and review permit it;
- hard timing caps: **45 minutes for R and 45 minutes for Q**, measured wall time excluding
  compilation/review but including preflights/settles; cap violations retain partial
  evidence and stop inconclusive. A planning estimate is roughly 40 minutes per phase at prior call times; these are
  caps, not promises or sampling targets. Typical durations may differ; no shortened samples
  or removed cells to meet a cap;
- no post-data instrument changes, alternate sampler, retries or product optimization;
- no automatic third design/benchmark cycle on this host if R or Q is inadequate.

Report the precise unresolved measurement limitation and stop. A different host or
instrument class would require a new decision and validation against the same budget;
instruction counts alone cannot certify a nanosecond budget. Nothing in this plan
publishes code, changes binding semantics, or authorizes relaxing acceptance.

## Why the sampling effort increased before execution

Claude's synthesis review estimated a robust within-cell paired-component SD around
0.93 µs in the prior 200 ms controls. A simple independent, approximately normal model
then estimates a seven-median component spread around 0.44 µs and suggests that an
all-pass rule across 72 eligible observations is likely to produce an inconclusive
result even before identity offsets. Those independence, distribution and scaling
assumptions are not established for the new method; its projected pass probabilities
are planning illustrations, not confidence claims. The richer control matrix cannot
responsibly assume that switching/data-sharing changes alone reduce this variation.

Choose 800 ms prospectively for the difficult AND 64 KiB rows to spend more observation
per cell, while retaining the seven-sample rule and all original budgets. Dropping
the redundant separate-data swapped configuration reduces 144 original coordinator
configurations to 96 in the final plan; it preserves the shared-versus-separate contrast
and keeps label crossover where it matters for the proposed common-data method.
The omitted separate-data crossover limits attribution inside that diagnostic regime.
Do not increase duration again after any new timing or infer small-row AND precision
from the prior single-credential cases.

The component-envelope completion rule is an explicit stricter validity requirement
in this proposal. It does not enlarge any budget. Matthew's later authorization to
execute this final plan should encompass it and the revised wall-time caps. This
current request approves design work only; no acceptance rule or result is applied
to the old evidence. A future executor must not treat this file as execution authority.

## Final design review disposition

Claude's focused reconciliation accepts this plan as ready for implementation review.
The coordinator accepts the fixed 45 minute caps and their limited overhead margin;
an overrun will stop inconclusive, without automatically raising the cap to 50 minutes.
Whole-cycle rounding can consume that margin. Since the algorithm targets accumulated
time, fewer cycles may be needed when calls slow down; elapsed phase time does not
scale linearly with a presumed percentage slowdown. No 90 percent success forecast or
specific host-speed tolerance is asserted.

The closed/candidate macro-schedule asymmetry is retained and disclosed. Prefix records
are included in the instrumentation capacity arithmetic. These final clarifications
change neither budgets nor sampling durations nor the accepted control matrix.
The original independent proposals and Claude's review retain their historical text.
