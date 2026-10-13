# Measurement-resolution study: independent design (Claude)

Author: Claude Opus 5.5 (Extra), 2026-10-13. Design only: nothing is built, run or implemented.

**Sources read:**
- `performance.md`;
- the prior investigation plan;
- the retained `performance-investigation` summary, provenance and raw blocks;
- `auth_measure.rs` and both runners, unchanged since `4437176`;
- my earlier reviews.

**Not read:** any coordinator, Astra, comparison or synthesis file in this directory.

**Authority unchanged:** every original row, each pair, exact S, differences of condition medians, `max(0, Da − D0) + 5000 + 2S`, the closed baseline `1.10B + 1000 ns`, and the >15% baseline spread rule. Original failures and the inconclusive diagnostic stay as recorded, and the slice stays blocked.

## 1. Question

**Phase R (resolution):** consider an instrument that:
- excludes the block-switch call from timing;
- fully balances first-order carryover;
- rotates the mapping from role to schedule slot and to construction order, rebuilding instances every sample.

Does it meet the **unchanged** ≤1000 ns per-observation null guard, plus a new null-bias bound, on the four AND 64 KiB rows?

**Phase Q (qualification):** only if Phase R is adequate. Does the unchanged registered matrix pass under that instrument, with an in-run adequacy sentinel?

Neither phase can regrade the historical runs or the earlier diagnostic.

## 2. What the retained data imply (descriptive, not causal)

From the raw blocks of the inconclusive diagnostic:

- **The 2 ms regime measured switching, not warm per-call cost.**
  - With about six calls per block, the anonymous adapter-minus-direct gap (`C0 − D0`) averaged +6.0 µs (range 3.8–10.0) across the balanced real rows, against +2.9 µs (range −1.8 to 9.2) in the contiguous arm. The frequent-switch regime added roughly 3 µs per call to the adapter path on average.
  - Per-call deviation by predecessor reached about ±1–1.8 µs.
- **The adapter-slot offset is stable within a process.**
  - The independent adapter-null per-sample mean `Ca_k − C0_k` was positive in 11 of 12 row/process cells, at about +0.5–1.6 µs, with per-sample SD of about 0.8–2 µs.
  - That pattern is consistent with an effect tied to the label slot or construction position, the last-constructed role being Ca.
  - Order-boundary carryover imbalance explains only about 0.1–0.2 µs of it.
  - The cause is unproven.
- **There were rare stall bursts.** 91 of about 64.5k blocks contained four or fewer calls, and environment capture was blocked.

The design targets these two identifiable instrument mechanisms directly. More samples alone would reduce variance but not a stable offset.

## 3. Instrument changes

Each change below departs from the registered method and must be recorded as a method decision before qualification. None changes a formula or budget.

### M1. Fixed-call blocks with one untimed lead-in call

Each block runs one untimed lead-in call, then K timed calls:
- **K = 8** for rows whose request or response is 64 KiB (about 2.8 ms timed, close to the prior granularity that cut null SD by 3–4×);
- **K = 256** for all other rows (about 2–3 ms at 8–12 µs per call).

K is fixed by payload class and is never calibrated from measurements. Every timed call does the complete registered per-call work. The lead-in is a complete call, excluded equally for every condition. The historical harness likewise excluded one warm-up call per sample. Lead-in durations are recorded but never used in a gate.

### M2. Cyclic carryover-balanced schedule

Repeat the 12-block cycle `S12 = [0,1,0,2,0,3,1,2,1,3,2,3]` over slots, with whole cycles only. Cyclically, including the inter-cycle 3→0 transition, every ordered pair of distinct slots occurs exactly once, and each slot appears three times. That makes the boundary imbalance found earlier impossible, and 12 is the minimum length that achieves it for four conditions.

### M3. Per-sample reconstruction with rotated mappings

Before each sample `k` (0–6) in process `p` (0–2), drop all four instances and build fresh ones. Two independent rotations are drawn from the cyclic Latin square `P_j = (j, j+1, j+2, j+3) mod 4`:

- **Role-to-slot mapping:** role `r` takes slot `P_{(k+p) mod 4}[r]`, with roles 0=D0, 1=Da, 2=C0, 3=Ca.
- **Construction order:** roles are built in order `P_{(k+p+2) mod 4}`.

The offset of 2 decouples construction position from slot. Across 21 process-sample cells per row, every role occupies every slot and every construction position 5–6 times.

After construction, each instance makes one untimed oracle/preflight call, which checks exact wire values and one effect, and warms its source. Construction, preflight and destruction are untimed and applied equally. This matches `performance.md`'s exclusion of initial caller construction.

### M4. Sample arithmetic

A sample for a role is the sum of timed nanoseconds divided by the sum of timed calls across its blocks within that sample. Run whole cycles until every role has at least 200 ms timed in the sample, which satisfies the registered minimum of 50 ms. Keep seven samples per role. Statistics use medians by role, not by slot.

### M5. First execution and environment

1. Each freshly copied binary first runs a `selftest` mode: one preflight call per condition, then exit. Its output is retained.
2. Then a fixed 10 s settle, with no load-conditional waiting.
3. Run without sandbox restrictions where permitted, so `sysctl` and `pmset` can be captured. Missing readings are recorded as missing.
4. The operator attests that no builds, tests or agents run concurrently. Load average is recorded before and after each process.

## 4. Phase R matrix

**Decision rows:** AND POST with a 64 KiB request and response, for {3.0.4, 3.1.2} × {binding, operation}. Each row runs three configurations, rotated by `(row + p) mod 3` so that each configuration takes each position four times:

1. **Real:** D0, Da, C0, Ca, the registered conditions, with S = 334/332.
2. **Independent null:** D0, D0\*, C0, C0\*. These are four separately constructed anonymous instances; the starred ones occupy the Da and Ca roles.
3. **Same-object null:** D0, D0≡, C0, C0≡. The aliased roles invoke the *same* instance as D0 and C0 respectively. This isolates schedule, slot and timing effects from instance effects.

**Attribution rows (never decision-bearing):** AND POST at 1 KiB for {3.0.4, 3.1.2} × {binding, operation}, real configuration only. These are the first AND 1 KiB data ever collected. They exist only to describe whether X depends on payload. They are not a substitute for, or a gate on, the 64 KiB rows.

**Processes:** three fresh processes of one binary.

**Estimated timed wall clock:** about 100 s per process, or about 5 min in total.

**Per row, process and configuration**, using medians of seven samples:
- `D = Da − D0`, `C = Ca − C0`, `R = C − D`, with the null roles in the Da and Ca positions;
- for real rows, also the frozen-formula ceiling and slack, which are diagnostic only.

**Descriptive only:** the per-sample paired differences (`Ca_k − C0_k` and the others) by median and range, the across-process mean and SD, and the slot/construction-position means implied by M3. These are disclosure, not confidence bounds.

## 5. Adequacy rule

Fixed before timing and blind to the real-auth rows. These are engineering checks, not statistical confidence bounds.

- **A1 (unchanged guard):** for each of the 12 independent-null observations (4 rows × 3 processes), `|D|`, `|C|` and `|R|` are all at most 1000 ns.
- **A2:** the same bound holds for all 12 same-object-null observations.
- **A3 (new bias bound):** across the 12 independent-null observations, `|mean D|`, `|mean C|` and `|mean R|` are each at most **300 ns**.
  - **Why it is needed:** A1 alone would pass a consistent +900 ns offset in every row, and that offset would shift the real gate silently.
  - **Why 300 ns:** it is about one third of the guard and about 5% of the AND-row allowance (5664–5668 ns). For 12 observations with per-observation SD up to 400 ns, it is about 2.6 standard errors. It would have caught the observed +744 ns adapter offset.
- **A4:** schedule, arithmetic and provenance validation pass, and every process exits 0.

**Outcome:** all pass gives `method-adequate`; anything else gives `inconclusive-stop`. Report the diagnostic pattern without regrading:
- A2 failing points to schedule or timing;
- A1 or A3 failing while A2 passes points to instance-level effects that survive rebuild and rotation.

## 6. Phase Q (one final qualification, only if adequate)

**The matrix:**
- all 40 original gates (24 unsecured and 16 incremental-auth);
- three fresh alternating process pairs, with the closed snapshot `abc9602` and the candidate;
- the identical sampler (M1–M5, K by payload class);
- seven samples of at least 200 ms each.

**Unchanged:** the closed binary uses four independently constructed anonymous roles under the same schedule and rotations. Formulas, exact S, the each-pair requirement and the 15% spread rule are unchanged.

**In-run sentinel, a validity condition rather than a budget:** each candidate process also runs the independent-null configuration on the four AND 64 KiB rows. If any sentinel observation violates A1, the matrix is **inconclusive (instrument)** whatever the gate outcomes, because adequacy must hold in the run that qualifies. Sentinels never adjust gate values.

**Resolution disclosure:** each auth pass reports its slack against that process's largest sentinel `|R|` and is labelled **resolved** (slack is larger) or **resolution-limited**. Pass or fail remains the registered formula. Completion cannot be claimed while any row is resolution-limited unless Matthew explicitly acknowledges it in the record.

**Outcome:**
- **Pass:** every gate passes and the sentinel and validation hold. The performance obligation is then met, subject to reviewer recomputation and the slice's other obligations.
- **Fail:** any gate fails.
- **Inconclusive:** spread above 15%, a sentinel violation or a validation failure.

Fail and inconclusive both leave the slice blocked.

## 7. Controls against cancellation masking

- Components D and C are bounded individually (A1, A3), so a small R cannot hide biased components. The direct null matters because a slow Da slot would inflate the ceiling and mask cost.
- The same-object null separates schedule effects from instance effects, so the two cannot offset each other unseen.
- Rotating role → slot and role → construction order every sample prevents any fixed slot or placement effect from aligning with a role.
- If any real row has `Da − D0 < 0`, which activates the `max(0, ·)` floor, report it.
- No null subtraction, paired-difference substitution, prepared-request-only baseline or payload substitution is used anywhere.

## 8. Provenance and integrity

The preregistration commit pins:
- the plan;
- `auth_measure.rs` and both runners;
- a fresh archive-qualification receipt showing only measurement inputs changed, with the runtime hash set equal to `3315307`;
- the closed snapshot.

**Phase R provenance** records those hashes, plus the binary hash and the `selftest` output. The binary is retained at a durable non-Git path.

**The Phase Q runner refuses to start unless:**
- Phase R's status is `method-adequate`;
- the measurement, runner, receipt and preregistration hashes all equal the ones recorded in Phase R;
- the summary declares the new sampler;
- it is run with normal Python, not `-O`. All validation uses explicit `SystemExit`.

**Before timing,** a validator self-test runs both runners' validation and arithmetic on synthetic raw output, both valid and deliberately corrupted. This is not a timing run.

## 9. Execution caps and abort criteria

- **Implementation:**
  - one change set to `auth_measure.rs`, the two runners and the self-test;
  - one correction round after instrument review;
  - then freeze. No change after any timing begins.
- **Phase R:** exactly one execution of three processes, with no reruns.
  - A setup or build failure before the first timing may be fixed once, since no data has been seen.
  - Any failure after timing starts is `inconclusive-stop`.
- **Phase Q:** at most one execution, only after Phase R is adequate *and* reviewers have recomputed it.
- **Stop rule:** if Phase R is inconclusive, stop wall-clock instrument work on this host. Any next step is a Matthew-level change of approach, for example a dedicated quiet host or an instruction-count instrument class. It is not another harness loop, and the budgets stay intact.

## 10. Reviewer checkpoints

1. Synthesis of this design and the coordinator's (now).
2. Instrument review before timing: the code, the self-test and the hash pins.
3. Phase R results: independent recomputation from raw blocks.
4. Phase Q results, if run: independent recomputation of all 40 gates and the sentinel.

## 11. Rejected alternatives

- **2 ms time-based blocks:** they time switching. The adapter path came out about 3 µs per call costlier on average, with carryover of about ±1–1.8 µs per call by predecessor.
- **Contiguous 50 ms blocks:** they are drift-dominated, with legacy null SD of about 2.8–3.7 µs.
- **Only more samples or longer accumulation:** these reduce variance but leave a stable slot or construction offset untouched.
- **Null correction, paired-difference gates, or payload substitution:** forbidden.
- **QoS pinning or hardware counters:** QoS is not available as affinity on macOS and changes the regime. Counters need privileges. A counter-based instrument is a possible future approach for Matthew to decide on, not part of this study.
- **Stopping now:** defensible. But the retained data identify two concrete instrument mechanisms that this design removes or randomizes, so one bounded attempt has a falsifiable rationale. If it fails, the stop rule applies.
