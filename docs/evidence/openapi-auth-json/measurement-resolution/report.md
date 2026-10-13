# Authenticated JSON measurement resolution

Date: 2026-10-13. The bounded control experiment ended **inconclusive-stop**. Independent GPT-6 Astra xhigh and Claude Opus 5.5 Extra reviews reproduce the result and agree that Phase Q must remain unstarted. The authenticated JSON slice remains performance-blocked.

## Result

The frozen plan required every eligible comparison to keep the direct difference, adapter difference, and their residual within 1,000 ns. **61 of 72 eligible comparisons passed; 196 of 216 component checks passed.**

| Control configuration | Passing comparisons | Role |
| --- | --- | --- |
| Same physical object under two labels | 20 / 24 | Required |
| Distinct objects sharing immutable fixtures | 22 / 24 | Required |
| The same distinct objects with labels swapped | 19 / 24 | Required |
| Distinct objects with separate fixture allocations | 10 / 24 | Diagnostic only |

Four same-object comparisons failed. The largest direct difference in those controls was 2,059 ns, despite both labels calling the same retained function object. Separate object or fixture allocation therefore cannot be the sole explanation. The experiment does not establish a machine, allocator, cache, or thermal cause.

The component envelope U_R was 3,927 ns for 3.0.4 binding, 5,166 ns for 3.0.4 operation, 2,351 ns for 3.1.2 binding, and 1,864 ns for 3.1.2 operation. These describe the observed ambiguity; they are neither confidence bounds nor corrections to authentication timings.

## Evidence and verification

The single control experiment completed all 96 configurations in three fresh processes. Its 107,808 raw records contain 672 untimed prefixes and 107,136 timed blocks, covering 6,856,704 complete calls. All row identities, mappings, schedules, minimum durations, capacities, and sample calculations validated. Wall time was **2,337.746 seconds**, below the unchanged 2,700-second cap.

Both reviewers accepted the instrument after one focused repair round, then independently recomputed the results from raw blocks. The frozen source, package receipt, compiler, binary identities, fixture table, and row checkpoints match. Before timing, nine Python tests passed normally and under optimization, three Rust harness tests passed against each SDK build, and candidate/closed readiness and checkpoint handshakes passed. The receipt extends the prior functional qualification for this new harness; it does not claim a fresh full functional rerun.

The reviewed freeze is commit `0cebee9`. No product runtime, client pin, public API, binding specification, budget, estimator, fixture, or sampler changed during the experiment. CPU/performance-level and thermal observations were unavailable; retained load observations do not establish a disturbance exemption.

- [Frozen execution plan](../measurement-resolution-design/synthesized-plan.md)
- [Recorded results](R/summary.json) and [provenance](R/provenance.json)
- [Coordinator recomputation](R/coordinator-verification.json)
- [Independent Astra review](R/astra-results.md)
- [Independent Claude review](R/claude-results.md)
- [Verified refusal to start Phase Q](R/Q-refusal.log)

## Stop disposition

The qualification runner refused Phase Q before creating its run directory. Historical numerical failures and the earlier inconclusive experiment remain unchanged. Phase R contains anonymous controls and cannot qualify authentication overhead.

The investigation is closed at its agreed stop. Sufficient resolution for the frozen difference-of-condition-medians estimator has not been demonstrated on this host. Any further host, instrument, or estimator change requires a separate decision and validation against the preserved budgets. The post hoc paired-statistic discussion in Claude's review does not change this result or authorize another run.
