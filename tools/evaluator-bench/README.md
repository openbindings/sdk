# Evaluator benchmark

This isolated executable compares SDK revisions with the same pinned release
compiler, profile, fixture generator and instrument lockfile. Its optional
unmodified jsonschema-rs 0.58.6 peer is an engine-cost reference: it does not
provide the SDK's exact decimal, four-outcome or bounded-work contract. The peer
and allocator instrumentation never enter the SDK dependency graph.

Build each revision with the **candidate's identical instrument directory and
lockfile**, saving separate binaries before the next build:

```sh
cargo build --release --locked --manifest-path tools/evaluator-bench/Cargo.toml
cargo build --release --locked --features allocations --manifest-path tools/evaluator-bench/Cargo.toml
python3 tools/evaluator-bench/test_policy.py
python3 tools/evaluator-bench/compare.py --baseline /path/base --candidate /path/candidate --baseline-alloc /path/base-alloc --candidate-alloc /path/candidate-alloc --output /path/results --enforce
```

Use `--workloads array` to replay a single workload. Use an unused output directory.
When a change alters the public API the instrument calls, the candidate's
instrument cannot build against the merge base; the workflow then reports the
pair as not comparable, passes the job and makes no comparison.
One fresh child reads one JSON request and emits one JSON-line observation. The
saved `fixtures.json` contains complete replay requests; add `mode` and
`iterations` from `commands.jsonl` to feed one directly to a binary. Every case
checks expected outcomes before timing (cold first use checks immediately after
its single timed preparation). Complete baseline/candidate observations must
agree, including refusal codes and original diagnostic locations. Every timed
iteration counts its actual outcome. A new refusal cannot pass as throughput.
The `reference-fan-in` fixture sends 100 distinct referring resources to one
supplied carrier with 5,000 unused definitions. Its `context` mode measures
public contract preparation; a counted unit test also requires one carrier
preparation and the unchanged traversal cancellation checkpoints.
The `reference-text-1000` and `reference-text-5000` fixtures repeat long reference
spellings to qualify preparation heap. The diagnostic allocator regression also
bounds cache storage growth when URI suffixes grow from 256 to 1,024 bytes.
Hash collisions are forced by the adapter's unit test; random benchmark hashes
are never treated as a deterministic collision fixture.

`eval` retains both contract and parsed instance; `context` retains the public
context but prepares a fresh contract. `compile` includes document proof,
resource context, preparation and destruction. `parse` includes destruction.
`full` includes preparation, value parsing, evaluation and destruction. `cold`
measures one first-use preparation in a fresh child, retaining its result until
after the timer. Other modes initialize fixed schemas before timing and run three
untimed operations. `baseline` carries the same fixture and retained owners but
only touches the value text. `--empty` measures an empty process without reading a fixture; its heap and RSS
replicates are retained alongside workload-carrying baselines. No baseline is
silently subtracted.

The driver runs three warmup children, adapts each variant to at least 200 ms,
then takes 20 adjacent randomized pairs per batch, twice. Cold first use always
uses one operation per fresh child. It reports median paired ratios and absolute
differences, 95% paired bootstrap intervals and MAD/median. Raw observations,
commands, binary and fixture hashes, compiler and host details are retained.
Requested System heap and allocation counts use separate instrumented binaries,
three fresh children per mode; they do not distort the timing binaries. Retained
baseline, total peak and incremental operation peak are all visible. RSS uses
three separate uninstrumented children and `wait4` where available; it includes
runtime and fixture carriage, not just evaluator memory.

The separate workflow **enforces outcome checks, requested heap and allocation
counts now**, using `--enforce`. Outcome mismatches, failed checks and heap/count
flags fail the job. Timing decisions and RSS remain advisory, including timing
regressions and inconclusive measurements. Omitting `--enforce` gives a local
advisory report without failing on outcome errors or heap/count flags.
Timing enforcement requires explicit review after about 20 calibration runs
show stable runner noise and acceptable false positives. Threshold/fixture edits
require performance review; the driver never updates accepted expectations
automatically.

Advisory timing decisions on shared runners use >20% median regression, lower 95% bound >1.20,
and an absolute increase >20 ns for small cases, >2 µs for preparation, or >10 µs
for large cases, confirmed in both independent batches. MAD/median >10%, an
interval spanning the threshold, or inconsistent batches is **inconclusive** and
requests a clean rerun without failing the job. Stable dedicated Linux x86-64 and arm64 runners can use
`--dedicated` for >10% / lower bound >1.10 after separate calibration. Dedicated
runner provisioning is outside this shared-runner workflow.

Requested heap flags require >10% **and** >128 KiB for any of retained baseline,
total peak, incremental operation peak or requested bytes per operation;
allocation counts require >5% **and** >32 per operation. Both thresholds must be
exceeded for the same metric. These flags fail enforced runs. RSS >15% **and**
>1 MiB requests dedicated verification and remains advisory. Ratchet improvements
per workload against the accepted revision.
Geometric means and SDK/peer ratios are dashboards, never substitutes for
per-workload gates or safe refusals. Do not run CPU-heavy tests beside timing.
