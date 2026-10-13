# Final review: bounded performance investigation results

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13.

## Scope

**Read:**
- the plan, the harness and both runners at the preregistration commit `4437176`;
- `performance-investigation/summary.json` and `provenance.json`;
- all three `process-*.stdout.json.gz`;
- `qualification-investigation/input-comparison.json` and `receipt.json`.

**Not done:** no builds, tests, benchmarks or reruns, and no Astra findings were read. All figures below are recomputed from the raw blocks.

## Verdict

- The preregistered stop rule fired correctly, and every reported number reproduces from raw data.
- **The bounded investigation can be closed.**
- **The authenticated slice remains blocked.** Both historical CPU runs remain failed, and the budgets are unchanged.

## Provenance

These hashes match the committed files byte for byte:
- the measurement source;
- the investigation runner;
- the preregistration;
- the frozen final runner (`qualification_runner_sha256`).

The receipt hash (`ccd8d47b…`) matches `qualification-investigation/receipt.json`. `input-comparison.json` lists only `auth_measure.rs` as a changed qualified input, with `productRuntimeChanged: false`.

All three processes exited 0, each in about 57 s, with no reruns. The committed final runner now pins the diagnostic's receipt and preregistration hashes, which closes my earlier integrity nits.

**Gaps:**
- The `candidate` binary is not retained in the output directory; only its hash is recorded.
- Environment capture was blocked: `sysctl` returned "Operation not permitted" and `pmset` failed. The CPU model, performance-core layout and thermal state are therefore unknown.
- Load averages of about 2.8–5.3 show a machine that was not idle. That is not a proven disturbance.

## Recomputed results

**Validation.** I independently re-validated:
- configuration rotation;
- Williams order and position for every balanced block;
- the minimum 2 ms per block and 200 ms (or 50 ms) per condition;
- each sample as `sum(ns) / sum(calls)`.

All 48 rows match `summary.json` to within 1e-6 ns.

**Balanced null guard: 3 of 12 rows pass, so the run stops inconclusive.** Only the 3.0.4 operation row passes, once in each process. Six rows fail on the adapter-null component, three on the direct component and five on the residual.

| Null statistic over 12 rows | Legacy 50 ms | Balanced 200 ms |
| --- | --- | --- |
| `Da − D0`: mean / SD / max abs | −278 / 2771 / 6959 ns | 96 / 850 / 1605 ns |
| `Ca − C0`: mean / SD / max abs | 498 / 2882 / 5959 ns | **744** / 836 / 1899 ns |
| residual: mean / SD / max abs | 776 / 3698 / 7604 ns | 648 / 828 / 1770 ns |

The balanced sampler reduced null variation about 3.3–4.5×. That is not enough for a ±1 µs component guard.

**Auth diagnostic rows (not qualifying):**
- **Balanced:** all 12 satisfy the unchanged formula, with minimum slack 442 ns (p1, 3.1.2 binding). `X = (Ca − C0) − (Da − D0)` has mean 2413 ns and SD 1261 ns.
- **Legacy:** 2 of 12 fail (p0 3.0.4 binding −2641 ns and p0 3.0.4 operation −663 ns), with X SD 3667 ns.

The minimum balanced slack is smaller than the balanced null variation, so the passing balanced rows establish nothing about the product. The summary states this correctly.

## Control disposition and honest limits

**The adapter null is systematically offset, not merely noisy.**
- The balanced `Ca − C0` null averages +744 ns, about three standard errors from zero.
- In the binding layer, all six rows are positive (mean ≈ +1.34 µs). In the operation layer the mean is about +0.15 µs.
- The direct-null mean (+96 ns) is consistent with zero.

The two adapter labels were doing identical, independently constructed work, so this is an instrument artifact.

**Exploratory, post hoc, and not decision-bearing:**
- **Carryover in the frequent-switch regime is large.** By predecessor, per-call deviations reach about ±1–1.8 µs: an adapter block that follows another adapter block is cheaper, and one that follows a direct block is costlier.
- **Boundary transitions explain little of the offset.** The disclosed boundary transitions (2→1, 3→2, 0→3, 1→0) imply only about +0.10 µs (binding) and +0.23 µs (operation) of adapter-null bias.
- **Most of the binding offset is unattributed.** Independent-instance placement or construction order is plausible but unproven, which is exactly the limitation the plan declared.
- **Stalled blocks were rare.** 91 of about 64.5k blocks contained four or fewer calls (median 6), clustered in p1 auth and p2 null rows. That is consistent with transient interference, and is not established as a disturbance.

**Implication.** The 64 KiB AND incremental-auth gate (allowance ≈ 5.67 µs) has not been measured with adequate resolution by either instrument. One reason is variance; another is a label-dependent bias of up to about 1.3 µs whose source is unknown. Neither the balanced passes nor the legacy failures say whether the product meets the budget. The historical failures are not explained away.

## Corrections to my earlier reviews

1. **Design opinion, point 2** ("The failures fit instrument resolution, not a runtime excess") was overconfident. The old data were *consistent with* a noise explanation. This diagnostic shows that my noise-only model missed systematic instance and label offsets. It also shows that no instrument here reaches the needed resolution. The historical failures remain unexplained and failed.
2. **The first-pair or first-launch hypothesis is not established.** Legacy failures recurred in p0, but balanced null failures occurred in every process.
3. **My phrase "null SD at least `allowance / 5`"** should read "at most". This was already acknowledged.
4. **My instrument review's projection held.** I said passing would need a per-quantity SD of about 0.3 µs; the observed balanced SD was about 0.83 µs, plus bias.

## Closure and next decision

**Close the investigation as `inconclusive-stop`.** Every preregistered bound was honoured:
- one experiment, with the full matrix completed;
- no threshold change, null correction, rerun or final matrix;
- runtime unchanged, and both failed runs retained.

The slice stays blocked on the incremental-auth gate.

**The smallest consequential next decision, for Matthew:** should the 64 KiB AND-POST incremental-auth row remain a hard acceptance gate exactly as registered?

- **If yes,** the slice stays blocked. Any further attempt needs a newly authorized, preregistered instrument investigation that addresses the construction-order and placement bias found here, for example:
  - counterbalancing label assignment and construction order across processes;
  - same-object nulls;
  - fewer switches per call.

  It must not be another measurement loop under the current design.
- **If no,** a recorded gate-design decision with independent rationale is required. It would decide, for future runs only, where incremental auth cost is resolved: for example, the existing formula on a low-payload AND row, which has historically had an SD of about 0.2 µs, with 64 KiB kept as a reported payload-independence diagnostic. Original results stay failed.

I recommend making this decision before authorizing any more measurement. I have not started either path.

## Addendum: factual corrections (no measurements)

1. **Binary retention: corrected.** The measured binary is retained outside Git at `/tmp/ob-auth-performance-investigation-20261013/candidate`. I hashed it read-only:
   - Its SHA-256 is `711571fe…c9171fae5`, identical to `provenance.json` `binary_sha256`.
   - The three `process-*.stdout.json.gz` files and `build.log` there are byte-identical to the evidence copies.

   The "binary not retained" gap in my Provenance section is withdrawn. The only remaining note is that `/tmp` is not durable storage. If the binary must remain available for later verification, move it to a durable, non-Git location.
2. **The low-payload AND extrapolation is withdrawn.** This experiment measured no AND 1 KiB row, and no historical AND 1 KiB data exists. The ~0.2 µs SD I cited comes from single-credential small-payload rows (bearer GET, key POST, large token) in the earlier runs. Neither the resolution nor the auth cost of a low-payload AND row is established.
3. **The binding-null offset is descriptive only.** The balanced binding adapter-null `Ca − C0` was positive in all six rows (mean ≈ +1.34 µs). Its cause is not established. Construction order, instance placement and any other mechanism remain unproven hypotheses. The carryover breakdown is an exploratory description, not an attribution.
4. **Next step: I align with the coordinator's recommendation.** Matthew asked for the budgets to stay intact, so the recommended next step is a future preregistered measurement-resolution design, not a gate change. It would include same-object nulls and controls for label and construction order, with every registered gate and budget unchanged.

   The second option in my "Closure and next decision" section (a recorded gate-design decision) is withdrawn as a recommendation. It was only ever a decision for Matthew, and he has given direction. The slice remains blocked, and both historical failures remain failed.
