# Phase R results: independent review (Claude)

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13.

**Method:** read-only, with my own Python recomputation from the raw gzip blocks. I did not import or call the runner's validator. No builds, timings, retries or source edits, and I did not read Astra's results.

**Authority:** `measurement-resolution-design/synthesized-plan.md` (`7fe8b446…`) and the freeze at commit `0cebee9`.

## Evidence reviewed (sha256)

| File | sha256 |
| --- | --- |
| `R/0-candidate.jsonl.gz` | `35eb7727f1d09a2363965876a22aff04732fdc06a872ab77aa0e9f7d1353ecad` |
| `R/1-candidate.jsonl.gz` | `6cdecf2130911a4bd5e80a30c61314d60c84fe311803ecb08f53d343659000d2` |
| `R/2-candidate.jsonl.gz` | `ce89f8385ba6e88c0863c2800307b79ef9eb80e6c41ab60fea66d89e30971af5` |
| `R/first-launch-candidate.jsonl.gz` | `4167c95113349f057e36ac86d25ae69e5add74179e885ccec93741cd2dcac63d` |
| `R/summary.json` | `96ebef2cef54b1f7589bf9f23ff497d26aa9c89cac58543c5992478554e67ae0` |
| `R/provenance.json` | `56c47a7c1b60c849c8ac594464caf85805b14eb83806c39698e4e713228b6f61` |
| `R/0-candidate-checkpoints.json` | `6b6d9c38…` |
| `R/1-candidate-checkpoints.json` | `cbf779d5…` |
| `R/2-candidate-checkpoints.json` | `c5f828f9…` |
| `R/first-launch-candidate-checkpoints.json` | `4cc93ed5…` |
| `freeze.json` | `ab7a4a89c56e05baaa18e21f80cfd780bc182b79821a8f84c196ffd9d9e00fbc` |
| `receipt-extension.json` | `ff6bd4ec…` |
| `fixtures.json` | `485905ad…` |
| `schedules.json` | `2716ed6a…` |
| `instrument-acceptance.json` | `3f994bfd…` |

The raw hashes equal `coordinator-verification.json`.

## Provenance and caps

- **The freeze is intact.** All 13 frozen files match `freeze.json`, and each is byte-identical to `HEAD` (`0cebee9`).
- **Binaries match the freeze.** Candidate is `e5f598e1…`, closed is `5df55527…`. The R-candidate copy used in every process record is `e5f598e1…`.
- **The compiler matches the freeze.** The supplied `rustc -vV` reports 1.99.0 (b940084d7 2026-09-28).
- **The provenance freeze hash** equals `freeze.json`.
- **Timeline** (from `start_ns`):
  - preflight-only first launch: 0.0–0.4 s, 4 rows, 0 timed blocks;
  - fixed 10.0 s settle;
  - p0: 10.4–783.5 s; p1: 783.9–1562.5 s; p2: 1562.8–2337.3 s.

  The phase ended at **2337.7 s, under the 2,700 s cap**. Every process exited 0 with no stop record.
- **Checkpoints:** for each process, the four row checkpoint sha256s equal the hashes of the raw lines.
- **Environment:** CPU and performance-level `sysctl` calls failed (exit 1) and thermal readings were unavailable, so those observations are missing. Load average ran from about 1.7 to 3.3. No disturbance is established.

## Independent recomputation

Across 3 processes × 4 rows × 2 cohorts × 4 configurations = 96 observations, my checks against the plan's own formulas found **0 problems**. They covered:
- **Row identity:** phase, process, row, edition, layer, workload, bytes, no error, not preflight.
- **Cohorts:** construction order (`(p+r)%2` reversal) and cohort set.
- **Physical objects** for each cohort:
  - ids 0–7 with distinct pointers;
  - ranks F or reversed;
  - direct versus adapter mode, all anonymous with `context_bytes` 0;
  - fixture ids, with input and response pointers shared exactly among ids 0–3 and distinct for 4–7.
- **Configuration identity:** IDs, maps and capacity 16,384.
- **Each sample's block structure:**
  - one untimed prefix (0 ns, K = 64) on slot `cycle[15]`;
  - whole 16-block cycles;
  - slot and predecessor matching the rotated or reversed `0010203112132233`;
  - calls = 64 and `object_id` = `map[slot]`;
  - every label at least 800 ms;
  - no extra cycle after the target was reached.
- **Arithmetic:** each reported sample equals `Σns / Σcalls`.
- **Chronology:** global ordinals are contiguous, every configuration-sample is uninterrupted, and the visit order equals the plan's cohort and configuration rotation.

**Totals:** 107,808 records (672 prefix, 107,136 timed), 6,856,704 timed complete calls and 2,310.79 s of timed work. Each sample took 9–10 cycles (mean 9.96). All of these equal the coordinator's figures.

**Medians and guards:** my D, C, R and U values equal `summary.json` for all 96 observations to within 1e-6 ns.

## Result against the frozen rule

| Measure | Result |
| --- | --- |
| Eligible observations passing (alias and common normal/swapped) | **61 / 72** |
| Component checks passing | **196 / 216** (failures: 9 D, 3 C, 8 R) |
| `U_R` (ns): 3.0.4 binding / 3.0.4 operation / 3.1.2 binding / 3.1.2 operation | 3926.6 / 5166.3 / 2351.3 / 1864.3 |

The eleven failing eligible observations:

| Process | Row | Cohort | Configuration | D (ns) | C (ns) | R (ns) |
| --- | --- | --- | --- | --- | --- | --- |
| p0 | 3.0.4 binding | R | common-swapped | −1012 | −227 | 785 |
| p0 | 3.0.4 operation | R | common-swapped | −307 | 2373 | 2680 |
| p0 | 3.1.2 binding | R | common-swapped | −1953 | −399 | 1554 |
| p1 | 3.0.4 binding | F | **alias** | −2059 | −1868 | 191 |
| p1 | 3.0.4 binding | R | **alias** | −1105 | −555 | 550 |
| p1 | 3.0.4 binding | R | common-normal | 1657 | 89 | −1567 |
| p1 | 3.0.4 operation | F | common-swapped | 682 | −574 | −1255 |
| p1 | 3.0.4 operation | R | common-normal | 1017 | −4149 | −5166 |
| p1 | 3.1.2 binding | R | **alias** | 1244 | −521 | −1765 |
| p2 | 3.1.2 operation | R | **alias** | 1486 | −378 | −1864 |
| p2 | 3.1.2 operation | R | common-swapped | 1401 | −16 | −1417 |

**Four of the failures are same-object (alias) controls.** By the plan's own reading, that shows inadequacy at the timing and schedule level, before any independent-instance effect. The diagnostic separate-data cells passed 10 of 24 and cannot rescue anything. Passes by cohort were F 34/36 and R 27/36, and by process p0 21, p1 18, p2 22 out of 24. These are descriptive patterns only.

## Descriptive observations (not causal, not a regrade)

- **Per-sample paired spread was lower than in the earlier diagnostic.** The robust SD of per-sample paired differences was about 0.25–0.36 µs (configuration medians), against about 0.93 µs in the earlier 200 ms diagnostic. Accumulation, block length, schedule and fixture sharing all changed between the two instruments, so this reduction is not attributable to 800 ms alone.
- **Common-level drift across samples was observed.** The across-sample range of the four-label mean within a configuration had a median of 7.9 µs and a maximum of 28.6 µs.
- **The frozen statistic is sensitive to which sample supplies each median.** It takes the difference of *separately selected* condition medians. For equally sized paired samples, that difference is bounded by the paired differences: `min_i(a_i − b_i) ≤ median(a) − median(b) ≤ max_i(a_i − b_i)`. It therefore cannot exceed every paired difference. However, common-level drift influences which samples supply each label's median, and the statistic can equal or approach a single atypical pair rather than the typical paired difference.
  - Example: p1 3.0.4 binding F alias drifts from about 330.7 µs to 345.8 µs over seven samples. Its per-sample paired D values are −0.20, −0.42, −0.12, −2.06, −0.34, −0.16 and −0.37 µs. Drift places both labels' medians in the same middle-in-time sample, so the median-of-labels D equals that one noisy pair, −2.06 µs. That is within the range of the paired values, not larger than all of them.
- **A paired statistic would nearly all pass, but that is not the rule.** Purely descriptively, medians of per-sample paired differences would satisfy the 1 µs checks in 71 of 72 eligible observations. That statistic is **explicitly excluded** by the frozen plan and `performance.md` ("no paired-difference substitution"). It is not used here, does not regrade anything, and adopting it would require a new recorded decision by Matthew.
- **The drift's source is not established.** I make no claim about machine, allocator, cache or thermal causes. CPU and thermal observations were unavailable.

## Disposition

The complete recorded controls **fail** the frozen adequacy rule: 11 eligible observations exceed the 1 µs guard, including four same-object controls.

**What follows:**
- The preregistered **inconclusive-stop** applies, and Phase Q must not launch. Without an adequate R summary and `review-acceptance.json`, the runner refuses Q.
- This closes the bounded measurement-resolution experiment. It does **not** qualify, and does not disqualify, the authenticated slice. The slice remains performance-blocked.
- Both historical CPU failures and the earlier inconclusive diagnostic remain as recorded. No budget, formula or guard changes.
- Under the plan's hard stop, there is no automatic third design or benchmark cycle on this host.

**The precise unresolved limitation:** with this instrument and host, the registered difference-of-condition-medians statistic did not hold all controls, including four same-object controls, within 1 µs.
- Common-level drift of several to about 30 µs across samples was observed. It can affect which samples supply each median.
- The measurements do not establish the drift's cause, and they do not show that drift explains every failure.

Any further step, such as another host, another instrument class or a recorded change to the statistic, is a new decision for Matthew.

*Correction note (2026-10-13), interpretive text only:*
- The median-difference bound replaces an incorrect claim.
- The spread reduction is no longer attributed to 800 ms alone.
- The limitation statement no longer asserts that drift dominates or explains the failures.

All counts, hashes, recomputed values, caveats about the excluded statistic and the disposition are unchanged.

FINAL R DISPOSITION: INCONCLUSIVE STOP
