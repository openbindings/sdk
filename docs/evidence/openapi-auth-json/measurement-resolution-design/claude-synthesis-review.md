# Review of the synthesized measurement-resolution plan

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Design review only; nothing was built, run or implemented.

**Read:** `coordinator-design.md`, `comparison.md` and `synthesized-plan.md`. I also analysed the retained diagnostic blocks read-only to check feasibility. Neither original proposal is revised.

## Verdict

**The synthesis is stronger than either original proposal.** It keeps:
- my execution and integrity mechanics: preflight-only first launch, validator self-tests, in-run sentinels and the hard stop;
- the coordinator's sharper identity design: same-object versus common-data versus separate-data instances, retained objects with label swaps, and no real-auth timing in Phase R, so the method cannot be selected after a favourable auth result.

**I found one material feasibility problem.** At 200 ms accumulation, the 72-observation all-pass adequacy rule is very likely to stop inconclusive from sampling variance alone, before any identity effect can be read (§2). There is a simple prospective fix. I also list six smaller inconsistencies (§3).

## 1. Points I concede

1. **My rotations were linked.** With slot `= r + t` and rank `= r − t − 2` (mod 4, `t = k + p`), a fixed role visits only 4 of the 16 (slot, rank) pairs. My "offset of 2 decouples" claim was wrong. Marginal balance held; independence did not. The synthesis's explicit cohort reversal plus label swap on retained objects is the right construction.
2. **The pooled 300 ns mean is unnecessary.** The per-row component envelope `U = max(|D| + |C|)`, compared with each gate's slack, addresses the risk I targeted (a consistent offset hiding under the 1 µs guard) at the point where it matters, and it does not cancel across layers or editions.
3. **No acknowledgement bypass.** My acknowledgement path was a discretionary waiver. Classifying a pass with slack ≤ U as inconclusive is more consistent with keeping the budgets intact.
4. **Fully timed 64-call blocks instead of per-block lead-ins are acceptable.** The retained blocks support this. The Allan deviation of block residuals, after removing mode and predecessor means, falls roughly as white noise up to about 30 ms (7.6 → 2.9 µs from 2 to 34 ms). It flattens and rises beyond that (3.6–4.5 µs at 67–270 ms). Blocks of about 22 ms sit inside the white region, so timing every call costs little resolution. Blocks should not get any longer.
5. **No 1 KiB AND attribution rows and no real-auth rows in Phase R.** They do not answer the instrument question, and removing real auth keeps the method decision blind.
6. **Retaining instances instead of rebuilding them is acceptable.** Rebuilding would have randomized placement. Retention instead makes placement attributable, and U carries its magnitude into Q. One limit remains unchanged: Q's real Ca object has one placement per process and cannot itself be swapped.

## 2. Material: the adequacy rule has almost no chance at 200 ms

**Observed noise.** Within the earlier balanced nulls, the per-sample paired difference between identical labels had a robust SD of about **0.93 µs** (median over 24 component series; IQR 0.49–1.32 µs) at 200 ms accumulation.

**Projected component SD.** A difference of two seven-sample medians then has an SD of about 1.25 × 0.93 / √7 ≈ **0.44 µs** per component. The residual R would be about √2 times that if D and C were independent.

**Probability of passing.** That gives approximately the following chance that every check passes, under independent-normal approximations that ignore the identity offsets the study is meant to find:

| Accumulation per label per sample | Component SD | R SD | Pass, one observation | Pass, all 72 observations |
| --- | --- | --- | --- | --- |
| 200 ms (plan) | ≈0.44 µs | ≈0.62 µs | ≈0.85 | ≈1e-5 |
| 400 ms | ≈0.31 µs | ≈0.44 µs | ≈0.975 | ≈0.16 |
| 600 ms | ≈0.25 µs | ≈0.36 µs | ≈0.995 | ≈0.67 |
| 800 ms | ≈0.22 µs | ≈0.31 µs | ≈0.999 | ≈0.91 |

The 200 ms numbers match the earlier result, where only 3 of 12 balanced null observations passed.

The 64-call blocks may lower per-sample noise somewhat by removing switching variance, but the data give no basis to assume a threefold gain. **As written, Phase R will almost certainly return `inconclusive-stop` for variance reasons.** It would spend its single allowed attempt without showing whether identity effects exist. The rule's strictness grows with the number of eligible cells. That is legitimate, but it has to be paired with enough measurement per cell.

**Prospective fix.** Set accumulation by row class before any data, exactly as K is set by payload class:
- **AND 64 KiB rows,** in Phase R and as Phase Q real rows and sentinels: at least **800 ms per label per sample**. This satisfies "seven samples ≥50 ms" and weakens nothing.
- **All other rows** stay at 200 ms. Their noise is about 0.2 µs, and the unsecured regression tolerance at 64 KiB is about 36 µs.
- **The closed and candidate binaries use the same accumulation for each row,** because the AND 64 KiB anonymous medians feed the unsecured regression gate.

**Cost and time caps:**
- **To stay economical,** drop configuration 4 (separate-data swapped). The causal common-versus-separate contrast survives through the normal mappings, and only the label-versus-object attribution *inside the non-proposed regime* is lost. Phase R then has 96 configurations at about 22.4 s timed each: **~40 min**.
- **Phase Q** comes to about 9.5 min per candidate process and 3.7 min per closed process, so **~40 min**.
- **Raise the caps** to R ≤ 45 min and Q ≤ 45 min, fixed now. The alternative, keeping 200 ms and the 25 min cap, preserves the schedule but makes the experiment uninformative.

**Caveat.** This projection rests on the retained 2 ms-block data and simple scaling, so it is approximate. It should be recorded as the design rationale before timing, and checked arithmetically at the instrument review. It must not be tuned after Phase R data exist.

## 3. Smaller inconsistencies

1. **Configuration IDs are reused across phases.** In R, 0 means same-object; in Q, 0 means real-auth and the sentinels reuse 1 and 2. Use separate namespaces in raw metadata and validators, for example `R0`–`R4`, `Q-real`, `Q-sent-normal` and `Q-sent-swapped`, so that no validator or reviewer can confuse them.
2. **The macro-schedule differs between phases.** R interleaves all configurations sample by sample, while Q runs each sentinel contiguously before or after its real row. The within-configuration statistic is unaffected, but adequacy is established under one schedule and checked under another, and slow drift between the windows is not sampled. Interleave Q's real and two sentinel configurations by sample round, as R does. It is the same code path, and it makes the sentinels contemporaneous.
3. **Q's construction list is unspecified.** F/R are defined for R's eight control objects. Q needs an explicit ordered list covering the four real objects and the four sentinel objects for each path, plus where shared fixtures are created.
4. **Closed-snapshot compatibility should be an explicit review item.** The new factory, schedule and alias code must compile and run against `abc9602`, which has no auth APIs, in anonymous-only form.
5. **The envelope rule changes outcome categories.** A numerically passing row with slack ≤ U blocks completion. That is stricter, not looser, but it changes what "qualified" means, so the preregistration should record Matthew's explicit approval of the rule rather than leave it implicit.
6. **The raw-capacity figure should be derived.** The 16,384-record reservation is ample: about 1,350 blocks per 800 ms configuration, and about 2,240 for small rows at K = 256. Derive the bound in the plan from K, accumulation and the cycle, so a later change cannot silently overflow it.

## 4. Remaining disagreements

- I would still prefer per-sample rebuilding for the *qualification* objects, to randomize placement bias in real Ca. Retention plus U is defensible, though, and simpler to attribute. I accept it.
- I am neutral on K = 64 versus a shorter K. The data do not favour either within the white region, so keep 64.

## Recommendation

Adopt the synthesized plan with three changes:
- the accumulation fix from §2 (800 ms for AND 64 KiB rows, dropping configuration 4, raising the caps);
- interleaving Q's real and sentinel configurations by sample;
- separate configuration-ID namespaces.

Record the remaining items from §3 at the instrument review. All of these are fixed before any timing. Budgets, formulas, the 1 µs guard, the historical failures and the hard stop stay as they are.

## Addendum: reconciliation of the revised synthesis

Read-only check of the changed sections of `synthesized-plan.md` and of `design-verification.json`. No other work.

**Verdict: ready for implementation review.** Every material item and every smaller item from this review is incorporated. The two notes below are non-blocking and can be settled when execution is authorized.

**Verified:**
- **800 ms targets.** The 800 ms target applies to the AND 64 KiB workload, its R controls and all its Q conditions and sentinels, including the closed anonymous conditions. It is selected from the frozen workload, never from speed or mode. Everything else uses 200 ms, matched between candidate and closed.
- **Phase R shape.** There are 96 configurations (configuration 4 dropped), with 72 eligible observations and 216 checks unchanged. The guard, the no-pooling rule and the inconclusive-stop rule are unchanged.
- **Minimum timed work.** Both figures are 2150.4 s:
  - R: 96 × 7 × 4 × 0.8 s;
  - Q: per pair, the candidate's 268.8 + 201.6 + 44.8 s plus the closed binary's 89.6 + 112 s, times three pairs.
- **Schedule balance.** The design check covers 672 expanded cells. It shows 42 per configuration per position, balanced cyclic edges and a valid reversal.
- **Record capacity.** The cap of 146 cycles per sample gives 16,352 records, within the 16,384 reserved. That is ample even for much faster calls, since AND 64 KiB needs 9–10 cycles and small rows about 17–25.
- **Q sample-round interleaving** of `Q-real`, `Q-null-normal` and `Q-null-swapped` uses the same policy as R.
- **Configuration namespaces** are unique, and validators reject IDs from the wrong phase.
- **The Q eight-object construction list** is explicit (`realD0`…`nullCB`, reversed for R), as is the closed or non-auth four-object list. Only the immutable fixture is shared.
- **The R-to-Q row mapping** (4, 5, 16, 17) matches the 24-row order.
- **Closed-snapshot compatibility** is now an explicit readiness requirement.
- **The envelope rule** is labelled a stricter validity requirement that Matthew's execution authorization must cover.
- **The planning model** is correctly caveated as illustration, not a demonstrated pass probability.

**Non-blocking notes:**
1. **The 45-minute cap has roughly a 10% margin.** Whole-cycle rounding at the 800 ms target is larger than the plan's 350 µs illustration suggests. At about 339 µs per call (the direct conditions), a label needs 10 cycles (868 ms), and the slowest-to-qualify label sets the cycle count for all four. That puts typical overshoot at roughly 8–10% for AND 64 KiB configurations. At prior call times, R's timed work is about 39 minutes before prefixes, preflights, settles and persistence, so roughly 40–41 minutes in all; Q is similar. A host slowdown of roughly 10% would breach the cap and end the phase inconclusive. Either accept that explicitly, or set the cap from a stated worst-case model (for example, 50 minutes) as part of execution authorization, before any timing.
2. **Closed and candidate auth rows use different macro-schedules.** Candidate auth rows interleave `Q-real` with two null configurations by sample round, while the closed rows run `Q-real` alone. Per-label medians come from identical blocks and cycles, and every sample starts with an untimed predecessor block, so this should not affect the unsecured regression gate (tolerance 10% + 1 µs). It is still an asymmetry, and the plan should state it rather than call the schedules identical.

Count the untimed predecessor block of each sample in the capacity arithmetic: seven more records per configuration, 16,359 in total, which still fits.
