# Independent Astra Phase R results review

Date: 2026-10-13. Reviewed the one Phase R matrix from frozen commit `0cebee9cb98164270938782525371ba7e9c6ed49`. Claude's results report was not read. This review ran read-only arithmetic and provenance checks; it did not run builds, benchmark processes, calibration, retries, or Phase Q.

**Accept the recorded inconclusive stop. Reject instrument adequacy for qualification.** All 96 registered observations are complete and internally valid, but only **61/72 eligible observations** and **196/216 component checks** pass. The plan requires every eligible observation and component to pass. Four same-object alias observations fail. Phase Q must remain unstarted, and the bounded investigation must stop without another method or measurement cycle on this host.

## Independent raw recomputation

The calculation was separately implemented for this review, without calling the runner's `validate_row`, `deltas` or `summarize_r` for its result. It read every block from all three compressed raw files and the retained first-launch file. It independently reconstructed the four-slot cycle, rotation/reversal, F/R construction and visit order, label-to-object mappings, samples, medians, differences and component envelopes from the frozen plan.

For each configuration/sample/label, it summed integer nanoseconds and full-call counts, excluded the prefix, computed the ratio as an exact rational number, and selected the fourth of the seven sorted samples. It then computed D, C, R and U from those medians. Every recorded sample equals the independently recomputed floating representation; all 96 reported median/difference/envelope results agree within 0.000001 ns. Guard decisions also agree under exact rational arithmetic. The closest eligible component to the 1,000 ns boundary is still 12.181120 ns away, so floating rounding cannot change this conclusion.

The independently reproduced counts are:

| Configuration | Passing observations | Eligible for adequacy |
| --- | ---: | --- |
| R-alias | 20/24 | Yes |
| R-common-normal | 22/24 | Yes |
| R-common-swapped | 19/24 | Yes |
| R-separate-normal | 10/24 | No; diagnostic only |

Of the 216 eligible component checks, D fails 9, C fails 3, and R fails 8. Eligible observations pass 21/24 in process 0, 18/24 in process 1, and 22/24 in process 2. Both F and R cohorts contribute failures; eligible cohort totals are 34/36 and 27/36 respectively. Neither pooling observations nor the separate-data cells can rescue the registered all-pass rule.

All 11 failed eligible observations are shown below. Process and row indices are zero based; F/R denotes construction cohort. Values are nanoseconds, rounded here to three decimals only.

| Process | Row | Cohort | Configuration | D | C | R |
| ---: | ---: | --- | --- | ---: | ---: | ---: |
| 0 | 0 | R | R-common-swapped | -1,012.181 | -227.411 | 784.771 |
| 0 | 1 | R | R-common-swapped | -306.527 | 2,373.080 | 2,679.607 |
| 0 | 2 | R | R-common-swapped | -1,952.700 | -398.616 | 1,554.084 |
| 1 | 0 | F | R-alias | -2,059.018 | -1,867.578 | 191.440 |
| 1 | 0 | R | R-alias | -1,105.096 | -555.210 | 549.886 |
| 1 | 0 | R | R-common-normal | 1,656.509 | 89.373 | -1,567.136 |
| 1 | 1 | F | R-common-swapped | 681.576 | -573.535 | -1,255.111 |
| 1 | 1 | R | R-common-normal | 1,017.333 | -4,149.006 | -5,166.339 |
| 1 | 2 | R | R-alias | 1,243.734 | -521.437 | -1,765.171 |
| 2 | 3 | R | R-alias | 1,486.150 | -378.108 | -1,864.258 |
| 2 | 3 | R | R-common-swapped | 1,401.349 | -15.689 | -1,417.038 |

The independent `U_R = max(abs(D)+abs(C))` calculation over all eligible observations in each row gives:

| R row | Workload | U_R, ns |
| ---: | --- | ---: |
| 0 | 3.0.4, AND 64 KiB, binding | 3,926.596094 |
| 1 | 3.0.4, AND 64 KiB, operation | 5,166.339063 |
| 2 | 3.1.2, AND 64 KiB, binding | 2,351.316406 |
| 3 | 3.1.2, AND 64 KiB, operation | 1,864.257813 |

These are empirical component envelopes, not error bounds, confidence intervals or offsets to subtract from an auth result. For example, process 1/row 0/F has a same-object residual of only 191.440 ns but component magnitudes of 2,059.018 and 1,867.578 ns. The component guard correctly prevents that cancellation from looking adequate.

## Completeness, identity and chronology

All three timed processes contain exactly four rows, both retained construction cohorts, all four configurations, and all seven sample rounds. There are **672 configuration samples**, **107,136 timed blocks**, and **672 untimed prefix blocks**. Timed blocks contain **6,856,704 full calls** at K=64; the 43,008 prefix calls are excluded from sample arithmetic. The first-launch output contains no timed blocks or sample values.

Every sample completes 9 or 10 whole cycles. Accumulated label durations range from **800,023,580 to 982,730,872 ns**, meeting the fixed 800 ms target. Each sample stops at the first complete cycle at which all four labels have met the target. Per-configuration record counts range from 1,063 to 1,127, below the 16,359 permitted records and the recorded 16,384 reserved capacity. No duration, cycle, record or completeness cap fired. Summed timed block duration is 2,310.789800156 seconds; the additional elapsed time includes the fixed settle, prefixes, preflights, persistence and other runner overhead.

Independent checks passed for:

- Exactly anonymous source/credential modes throughout R, with no authenticated object or context byte charge. Full 64 KiB fixture input/response contents and complete source/context/input/response hashes match the frozen fixture table.
- Stable object IDs, constructor ranks, common-versus-separate fixture identity, distinct physical closures, and independent retained F/R objects. Alias labels map to the same object on each path; swapped labels map to the same retained pair in the opposite order.
- Every block's full-call count, object and fixture identity, sample/cycle/position, prefix flag, cyclic predecessor and slot. Every cyclic sequence includes all 16 ordered predecessor pairs exactly once, including self pairs and wraparound.
- Row-global ordinals form a continuous zero-based sequence. Samples for a configuration remain uninterrupted, and complete visits match the registered cohort/configuration rotation and reversal order.
- All 16 persisted checkpoint hashes, including the four first-launch rows, match their exact raw JSON lines and the provenance copies. Compressed and uncompressed raw files are byte-identical after decompression. The retained stderr files are empty. The frozen runner's acknowledged barrier is consistent with the complete checkpoint sequence.

These validate the recorded chronology and its linkage to the reviewed binary. The raw blocks record durations and ordinals rather than absolute start/end timestamps for every block; they do not provide an independent scheduler trace.

## Provenance and phase cap

All 13 inputs named in `freeze.json`, plus the freeze itself, match the corresponding bytes in commit `0cebee9cb98164270938782525371ba7e9c6ed49` and the current worktree. The frozen compiler identity matches live read-only `rustc -vV`. The qualified extension still matches its prior receipt, closed snapshot, both consumer inventories and all 1,353 recorded instrument/readiness/package/functional/closed-source hashes. Both frozen binaries match their receipt hashes; the launched `R-candidate` copy matches the frozen candidate binary. The current tracked worktree has no instrument, runtime, plan, fixture or historical-result modifications.

The provenance records one preflight-only first launch followed by exactly three timed candidate processes in order 0, 1, 2. All four child processes exited 0, with no interruption or watchdog entry. The coordinator's reported outer-runner exit 2 is consistent with the frozen runner returning 2 for `inconclusive-stop`; the provenance records the final status rather than the outer shell exit code itself. This was an adequacy failure after a complete experiment, not a child-process failure.

Recorded phase start/end are **2026-10-13 14:45:06.827643 UTC** and **15:24:04.573587 UTC**. Their independently computed wall difference is **2,337.745944 seconds**. The recorded monotonic elapsed value is **2,337.731353125 seconds**, leaving **362.268646875 seconds** under the 2,700-second cap. The wall/monotonic difference is 0.014591 seconds and does not affect the cap outcome. Absolute monotonic endpoints are not retained, so the monotonic difference cannot be separately reconstructed from two stored monotonic timestamps; it is cross-checked by the wall record and the reviewed measurement code.

Process before/after wall observations are ordered and nonoverlapping. The interval between the end observation of the first-launch preflight and the start observation of process 0 is 10.037716 seconds, consistent with the fixed 10-second settle plus overhead. Timed-process wall intervals are 773.069950, 778.580992 and 774.504816 seconds. Their declared remaining budgets are positive and within the same phase deadline. Neither a cap exception nor a shortened measurement explains the stop.

CPU model and performance-level queries failed with permission errors. The thermal command returned exit 0 but its text reports failures for thermal/performance warnings and CPU power status; those observations are **unavailable**, not evidence of a cool or stable machine. Load-average snapshots are retained but cannot identify the cause of the control variation. The evidence does not justify attributing a failure to thermal state, core migration, allocator placement, cache behavior, scheduler interference or a particular background process.

## Interpretation and stop decision

The same-object failures establish inadequate resolution under the registered method even when the compared labels borrow the same physical closure and its immutable fixture. Different physical object or fixture allocations therefore cannot be the sole explanation of the failures. The distinct/common and separate-data observations provide controlled associations, but their pass-rate differences do not prove a machine-level cause or explain away previous failures. The four alias failures alone are sufficient to reject adequacy; the additional eligible failures reinforce the same registered decision.

Phase R contains no real-auth timing and does not measure or qualify auth overhead. It supplies no passing result for the original unsecured or incremental-auth gates. Historical failed outcomes remain failed. There is no `Q` phase directory or `Q-candidate` phase copy, and the frozen Q entry gate requires `method-adequate-for-one-qualification`, which this R summary does not have. An acceptance of this review's stop decision must not be encoded as adequacy or used to launch Q.

The authorized bounded investigation has reached its prescribed inconclusive stop. No retries, post-data instrument changes, alternate sampler, null subtraction, relaxed guard, or automatic third design/benchmark cycle follow from this result. A different host or instrument class requires a separate decision under the unchanged budgets. Overall authenticated-slice performance completion remains blocked.

## Exact reviewed identities

| Evidence or artifact | SHA-256 |
| --- | --- |
| `freeze.json` | `ab7a4a89c56e05baaa18e21f80cfd780bc182b79821a8f84c196ffd9d9e00fbc` |
| `receipt-extension.json` | `ff6bd4ecdd698caaee9fbba9fdb34c7064cf555e2beade9374c569b0b20aec59` |
| `auth_resolution.rs` | `959ec5a0db047b9975c1d5d4f20b79685975e795c32ba28983fc89c614f17974` |
| `tools/run_auth_resolution.py` | `02585376b402292c419c5b9c8eadbc13172be0cce7f05f5d5138a2afb6296f71` |
| `tools/auth_resolution.py` | `44867c81fcce03428e551585a13e8bbc0c7e7f4b374909abf1b082b213880625` |
| Frozen candidate and launched R candidate binaries | `e5f598e1e54eda1eaa7c8169ac688028aaa50b90834a126eaf957b971e9284d7` |
| Frozen closed binary, unused in R | `5df55527f3ab3a083777f36618738a10defdc4eb8468915726d2a8c0c8006e6e` |
| `R/first-launch-candidate.jsonl.gz` | `4167c95113349f057e36ac86d25ae69e5add74179e885ccec93741cd2dcac63d` |
| `R/0-candidate.jsonl.gz` | `35eb7727f1d09a2363965876a22aff04732fdc06a872ab77aa0e9f7d1353ecad` |
| `R/1-candidate.jsonl.gz` | `6cdecf2130911a4bd5e80a30c61314d60c84fe311803ecb08f53d343659000d2` |
| `R/2-candidate.jsonl.gz` | `ce89f8385ba6e88c0863c2800307b79ef9eb80e6c41ab60fea66d89e30971af5` |
| `R/first-launch-candidate-checkpoints.json` | `4cc93ed5df64682a9a15d4acc3fa3a4ee89b6732702ff9fe125d61f217a11be3` |
| `R/0-candidate-checkpoints.json` | `6b6d9c38a4e0c552d8d50f6edd6dcad61c94ad24feec5ac264122a0a3e943623` |
| `R/1-candidate-checkpoints.json` | `cbf779d53e5cb74ec90ddb1ac6dc9285487ad24e5910236bbc94e28a9a23f7be` |
| `R/2-candidate-checkpoints.json` | `c5f828f9551917b59b0f605342bb322fea110d2d6140fbabf4c17beabc7fdf26` |
| `R/summary.json` | `96ebef2cef54b1f7589bf9f23ff497d26aa9c89cac58543c5992478554e67ae0` |
| `R/provenance.json` | `56c47a7c1b60c849c8ac594464caf85805b14eb83806c39698e4e713228b6f61` |

FINAL R DISPOSITION: INCONCLUSIVE STOP
