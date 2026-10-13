# Instrument review: preregistered performance investigation

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13.

## Scope

**Read, in their current on-disk state:**
- `performance-investigation-plan.md`, including the added note on boundary transitions;
- the `auth_measure.rs` diff against `3315307`;
- `tools/investigate-openapi-auth-performance.py`, including the added per-component standard deviations;
- the unchanged `tools/measure-openapi-auth-json.py`, for the final-matrix handoff.

**Not done:** no builds, tests or benchmarks, and I did not read any Astra report.

## Verdict

I found no correctness defect that would invalidate the diagnostic. One item should be fixed **before timing starts**, because it affects the only final matrix the plan permits (item 1 below). The other notes do not block the run.

## Correction to my design opinion

The plan is right. In `claude-investigation-design.md`, "null SD at least `allowance / 5`" should read "**at most** `allowance / 5`". No decision in this plan depends on it.

## Flag before timing

**1. The final-qualification driver does not yet select the balanced sampler.** `tools/measure-openapi-auth-json.py` launches each binary with only `anonymous-only` and `reverse`. Without `balanced`, both binaries would silently run the legacy 50 ms sampler. The driver also builds both binaries and runs pair 1 immediately, with no settle step.

Make the following change now and record its hash with the preregistration, so it cannot be shaped after the diagnostic outcome is known:
- pass `balanced` to both binaries;
- apply the same fixed settle step the diagnostic uses, if that is intended for the final matrix;
- leave the formulas, the spread rule and the matched-source check exactly as they are.

## Verified by reading

- **The Williams square is valid.** Each condition takes each position once. The 12 within-order adjacent pairs are exactly the 12 distinct ordered pairs. Row-wise reversal under `reverse` keeps both properties.

  The added boundary transitions are 2→1, 3→2, 0→3 and 1→0. They duplicate one predecessor per condition per cycle. This is now disclosed, it is visible in the raw schedule, and it is a second-order effect that the null controls partly expose.
- **Configuration rotation is correct.** `(row + process) % 4` places each configuration in each position three times. The validator rebuilds the expected order independently. `row_index` counts only the AND rows, because other workloads are skipped before the layer loop.
- **The sample arithmetic is correct.** Each balanced sample is total nanoseconds divided by total calls across whole 16-block cycles, with at least 200 ms per condition and at least 2 ms per block. The validator recomputes every sample exactly, including legacy order alternation and the minimum durations.
- **The null controls are constructed correctly.** All four labels do anonymous work on the same unsecured source, built independently. `context_bytes` is 0, so the diagnostic ceiling for null rows is informational only.
- **Real rows do the same work as the registered harness,** with S equal to 334/332 bytes.
- **The preflight change is limited to the diagnostic.** It replaces per-sample warm-ups in both arms there, as declared. The non-investigation legacy path keeps the historical per-sample warm-up and round alternation.
- **The closed binary in balanced mode is built as intended.** It constructs four anonymous conditions: `authenticated` excludes `anon_only`, and the skip applies only to legacy mode. The two binaries therefore share the balanced schedule.
- **The decision uses only balanced null rows.** Legacy null rows are reported for comparison. There is no null correction and no early stop before the full matrix completes.

## Non-blocking design notes

- **A 2 ms block holds about six calls at 64 KiB.** That is roughly 345 µs per call, so the first call after each switch is about one-sixth of every block. The balanced arm therefore measures a frequent-switch regime.
  - Differences remain interpretable, and the null controls detect switch costs that differ between identical instances.
  - Absolute balanced medians are not comparable with legacy or historical medians, and the report should say so.
  - At 1 KiB, a block holds roughly 200 calls, so this matters only for the large rows.
- **The `blocks` vector grows between blocks.** It reaches about 2,800 entries per configuration, so it reallocates between timed blocks, which is outside timing but disturbs caches. Pre-reserving would remove this. The disturbance is small and spread across positions.
- **The fixed 10 s sleep does not cover the first execution of the freshly copied binary.** That cost is the most plausible source of the earlier pair-0 noise. If you want to mitigate it, the place is a uniform, predeclared untimed first execution, decided before timing. Otherwise, accept that process 0 may trip the guard; an inconclusive stop is an honest outcome.
- **Strictness of the 1 µs guard.** All 36 checks (12 balanced null rows × 3 quantities, as maximum absolute values) must pass. That effectively needs a per-quantity SD of about 0.3 µs or less to have a good chance of passing, which is much stricter than my proposed SD rule. It makes an inconclusive stop more likely. It is prospective and conservative, though, and the component checks catch instance-placement biases that a residual-only rule would hide. I consider it a legitimate choice.
- **The smaller matrix is enough for the declared decision** (instrument adequacy, then one final matrix). It gives up two things:
  - With no same-object null, a guard failure cannot be attributed to sampling noise versus placement.
  - With no AND 1 KiB or single-credential 64 KiB rows, a failure in the final matrix will carry no causal evidence about whether auth cost depends on payload.

  Both are acceptable losses for a bounded run, provided they are reported as limits rather than filled in by inference.

I will review the measured diagnostic results and, if the guard passes, the final matrix.

## Addendum: final-driver delta (before timing)

**Read-only scope:** the `git diff` of `tools/measure-openapi-auth-json.py` against `3315307`, the provenance keys in `tools/investigate-openapi-auth-performance.py`, and the plan's final-runner and limits paragraphs. No builds or benchmarks; no diagnostic timings exist yet.

**Verdict: accepted.** The earlier flag is resolved. One integrity nit below is worth closing before timing, because the diagnostic hashes this driver; none blocks.

**Verified:**
- **The balanced sampler is reached only through an adequate diagnostic.** `--balanced-qualified-by` requires `status == method-adequate-for-one-qualification` and all 12 balanced null rows passing.
- **Both sources are pinned to the diagnostic.**
  - The measurement-source hash must equal the hash recorded in the diagnostic provenance.
  - This driver's own hash must equal `qualification_runner_sha256`, which the diagnostic now records. Any edit to the driver after the diagnostic therefore refuses to run.
  - The existing closed/candidate source-equality check is retained.
- **Both binaries get `balanced`.** `anonymous-only` (closed binary) and `reverse` (odd pairs) are unchanged, and the fixed 10 s settle runs once after the builds. That matches the diagnostic.
- **Raw validation matches the harness.** It checks 24 rows, the balanced sampler and no null rows, plus per sample:
  - whole 16-block cycles;
  - `(cycle, order, position)` order, with `orders[order][3 − position]` for reverse pairs, which matches the harness's `reverse` indexing;
  - blocks of at least 2 ms with calls;
  - at least 200 ms per condition;
  - exact `ns / calls` sample arithmetic.

  Each process's stdout is saved before validation, so a failed validation leaves its raw output in place.
- **Gate formulas, the spread rules and the exit status are unchanged.** `runpy.run_path` loads `environment` without running the diagnostic's `main`.

**Integrity nits (non-blocking):**
1. **Omitting the flag silently runs the legacy sampler.** The plan says the final runner "requires" `--balanced-qualified-by`, but the flag is optional. Either make it mandatory for this investigation (keeping legacy behind an explicit legacy-reproduction flag) or check `summary.sampler == 'balanced-200ms'` as part of acceptance.
2. **The final run is not tied to the same candidate or plan.**
   - Comparing `method_provenance['receipt_sha256']` with the current receipt would ensure the final matrix measures the candidate qualified for the diagnostic.
   - Comparing `preregistration_sha256` with the current plan file would ensure the plan has not been edited in between.

   Both are cheap and enforce "no intervening runtime or plan change".
3. **One summary note is stale for balanced runs.** It still says "seven >=50ms interleaved samples". The `sampler` field disambiguates, but the note should name the sampler.
4. **Validation uses `assert`, which `python -O` strips.** Run both scripts without `-O`, or convert the checks to explicit `SystemExit`s.
