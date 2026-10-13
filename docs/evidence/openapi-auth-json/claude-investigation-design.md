# Investigation design opinion: AND 64 KiB incremental-auth failures

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. SDK commit `3315307`.

## Scope

**Read:** `performance.md`, the `cpu-run1` and `cpu-run2` `summary.json` and `raw.json`, the evidence `README.md`, `auth_measure.rs` (unchanged from the committed tree) and `tools/measure-openapi-auth-json.py`. I reviewed `security.rs` and `invocation.rs` earlier in this session.

**Not done:** I ran no builds, tests or benchmarks. All statistics below are recomputed from the retained raw files. Both failed runs remain failed.

## What the retained data says

**Gate statistic.** I computed the statistic the gate implicitly tests per pair, `X = (Ca − C0) − (Da − D0)`, the adapter-specific auth cost, across both runs. There are 24 values per workload (2 runs × 2 editions × 2 layers × 3 pairs).

| Workload | C0 total | mean X | SD X | allowance (5000 + 2S) |
| --- | --- | --- | --- | --- |
| bearer GET 1 KiB | ~9 µs | 1053 ns | 214 ns | 5442 ns |
| key POST 1 KiB | ~12 µs | 1037 ns | 249 ns | 5430 ns |
| large token | ~9 µs | 1072 ns | 239 ns | 13506 ns |
| AND POST 64 KiB | ~351 µs | 2093 ns | 2731 ns | 5668 ns |

**1. The adapter's auth cost scales with the number of credentials, not with payload.** That cost is about 1.05 µs for each single-credential workload and about 2.1 µs for two credentials, with no layer difference: operation 2.04 µs, binding 2.15 µs. The two-credential estimate is noisy (SD 2.7 µs), but it is consistent with the single-credential rows, which have SD under 0.25 µs. I found no auth-path work proportional to body size:
- `security::apply` touches only the context and source metadata;
- the client adds only header and credential accounting;
- body parsing and the duplicate walk are identical in both adapter conditions.

**2. The failures fit instrument resolution, not a runtime excess.** At 64 KiB, each 50 ms sample varies by about 0.5–1% of a ~345 µs total, roughly 2–3 µs. The gate takes a difference of differences of four medians, which yields an SD of about 2.7 µs against a margin of about 3.6 µs (allowance minus the ~2 µs true cost). Under normal approximation, that gives roughly a 10% exceedance per test. A run has 12 AND tests, so roughly one false failure per run is expected; the runs observed one and two.

Even the direct path's own auth difference `Da − D0`, which involves no adapter, ranges from 0.9 to 6.7 µs across pairs at 64 KiB, compared with ±0.07 µs at 1 KiB.

**3. The first process pair is markedly noisier.** For AND rows, the SD of X by pair is 4.19 µs for pair 0, 1.99 µs for pair 1 and 1.46 µs for pair 2. The means are similar: 2.8, 1.6 and 1.9 µs. All three failures occurred in pair 0, the first execution after both release builds. That is consistent with a transient first-launch or post-build disturbance, such as first-run executable assessment, page-cache warmup, background indexing or a thermal transient. No such disturbance was documented, so it cannot be used to excuse a failure.

## On the stated hypothesis

The hypothesis is mostly right, but it needs one refinement. The conditions are already interleaved within a process; the issue is that every per-condition median is a separate draw with about 1% noise, and the subtraction of two large totals turns that into absolute noise comparable to a fixed allowance. Two levers address different noise components:
- **Short-block balanced interleaving** turns slow drift (frequency, thermal, background load) into common-mode noise, which cancels in the differences.
- **Longer accumulation per sample** reduces white noise. `performance.md` requires samples "of at least 50 ms", so longer samples stay within the registered wording.

## Recommended diagnostic run (one, predeclared)

Commit the following before the run:

1. **Two samplers, both computed with the exact frozen gate formulas:**
   - **(A)** the current sampler: contiguous 50 ms blocks, alternating round order;
   - **(B)** balanced short blocks. Use fixed iteration counts per block, at least about 20 calls at 64 KiB, in a mirrored order (D0 Da C0 Ca, then Ca C0 Da D0) within each cycle. Assign cycles round-robin to the seven samples, so that each sample accumulates at least 50 ms (optionally at least 200 ms) of work per condition spread across the whole row. Keep the one untimed warm-up call per block only if blocks are at least about 20 calls, and declare this either way.
2. **Null (A/A) controls:** add duplicate conditions C0′ and D0′ to the AND 64 KiB rows (both editions and layers) and to one 1 KiB row. Run them two ways:
   - the same function object under a second label, which measures pure sampling noise;
   - a separately constructed identical instance, which also captures memory-placement effects.

   Evaluate them with the exact gate expression, `(C0′ − C0) − max(0, D0′ − D0)` against `5000 + 2S`, and report the exceedance fraction and SD under each sampler. The nulls are diagnostic only and never adjust a threshold.
3. **A low-noise isolation row:** AND POST at 1 KiB, plus bearer and key at 64 KiB. These test directly whether the adapter's auth cost is independent of payload, which is the substantive question.
4. **Environment record:** CPU model and performance levels, load average and thermal state before and after each process. Also add one uniform pre-declared settle step: execute both binaries untimed once before the first gating pair, then wait for the load average to settle. The three gating fresh pairs stay unchanged. This addresses the pair-0 effect without discarding data.
5. **Sampler adoption rule, fixed in advance and blind to candidate outcomes:** adopt B for the final matrix only if its null SD is lower than A's on all four AND null rows, and its null SD at least `allowance / 5` (about 1.1 µs), so that a true cost of about 2 µs has a margin of at least three SD. If neither sampler meets this, the 64 KiB incremental gate is under-resolved. The slice then stays blocked pending a recorded gate-design decision, for example resolving incremental cost on the low-noise AND 1 KiB row while keeping 64 KiB as a payload-independence check. The executor may not make that change.

The final qualification must then use the chosen sampler for **all** rows and both binaries, with three fresh pairs and unchanged formulas. Any failure or inconclusive result stays blocked.

## Runtime repair: not justified by the evidence

The redundant work I can find in the auth path is small and constant:
- the server origin is parsed again on every attempt;
- `analysis::object` builds a second duplicate-check `HashSet` for the security point after `has_duplicate_names`;
- grant origins are parsed by the adapter and again by the client. The client check is the final authority, so that part is intended.

Together these are well under a microsecond and cannot fix a ~2.7 µs SD problem. Spending the one allowed focused repair on them would look like chasing green. Use that repair only if the 1 KiB AND row shows a real excess, which the current data does not suggest.

## Other cautions

- **Neither run's result changes.** Run 1 and run 2 failed under the registered instrument. A better instrument does not reinterpret them.
- **The inconclusive rule does not see this noise.** For incremental rows, the 15% spread rule looks only at the direct medians across pairs, so it cannot detect delta noise. Report the candidate spreads and the null SD next to each row. Any new resolution-based "inconclusive" rule would itself be a gate change needing a recorded decision.
- **The regression gate's two binaries interleave differently.** The closed binary interleaves two conditions (`anonymous-only`) and the candidate interleaves four. That asymmetry is acceptable for the regression gate, but whichever sampler is chosen should keep it identical between the two binaries.
- **Completion:** the slice cannot be called complete until the final predeclared matrix passes. "Noise" is an explanation for these failures, not a pass.
