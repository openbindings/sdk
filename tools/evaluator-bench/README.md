# Evaluator benchmark

This isolated executable compares SDK revisions with the same pinned release
compiler, profile and fixture generator. Ordinary pairs share an instrument
lockfile; cross-API pairs may change only SDK-owned lock entries. Its optional
unmodified jsonschema-rs 0.58.6 peer is an engine-cost reference: it does not
provide the SDK's exact decimal, four-outcome or bounded-work contract. The peer
and allocator instrumentation never enter the SDK dependency graph.

All SDK calls and SDK types live in `src/sdk.rs`. Opaque owners expose a fixed
internal interface for value parsing, byte document validation and proof,
conformant reads, a reusable preparation context, input preparation, cheap
validation and explanation. Fixtures, checks, timers, the peer and allocation
accounting use only that interface and ordinary Rust/JSON data.

Build both release variants before measuring. The build driver first builds the
candidate's identical instrument and lockfile against both source trees:

```sh
python3 tools/evaluator-bench/test_policy.py
python3 tools/evaluator-bench/compare.py build --baseline-source /path/base-source --candidate-source /path/candidate-source --output /path/results --binaries /path/bin --target /path/build
python3 tools/evaluator-bench/compare.py --baseline /path/bin/baseline --candidate /path/bin/candidate --baseline-alloc /path/bin/baseline-alloc --candidate-alloc /path/bin/candidate-alloc --output /path/results --enforce
```

Use clean source snapshots at the same pinned compiler. Each snapshot's SDK
sources must be at their usual relative paths. Every build variant (each side's
timing and memory binaries, and any fallback) gets its own new directory under
`--target`, and the driver refuses to reuse one: Cargo fingerprints the
instrument by paths relative to its own workspace and judges archived sources
with old modification times fresh, so a shared directory could hand one variant
another's binary. `build.rs` embeds digests of the instrument sources and of the
SDK tree it compiles against, and `--provenance` prints them. The driver
recomputes both from the trees it claims to build and fails the build when a
binary reports others, naming the candidate tree if the baseline binary carries
its SDK sources. `instrument-pair.json` records every build's directory, trees,
digests and binary hash. Direct release builds remain
available with `cargo build --release --locked --manifest-path
tools/evaluator-bench/Cargo.toml`, adding `--features allocations` for the separate
memory instrument. `cargo test --locked --manifest-path
tools/evaluator-bench/Cargo.toml` checks the executable's observations.

If either identical baseline release variant fails, separate instruments are
allowed **only** when the entire instrument diff is confined to `src/sdk.rs` and
`Cargo.lock` package blocks for the SDK's own workspace crates. Third-party lock
entries, lock metadata, fixtures, timing, checks, manifests, allocation code and
all other instrument edits prohibit that fallback. Each side then builds its
own instrument and lockfile. The driver saves `instrument.diff`, its SHA-256,
both lock hashes and the `cross-API` label in `instrument-pair.json` and carries
that record into `comparison.json`. Cross-API pairs use exactly the ordinary
outcome, heap, allocation-count and timing policy. Other instrument differences
remain **not comparable** if the identical build fails; no measurements run.
Candidate build failures and failures of an authorized baseline fallback fail
the job. Only the root `target`/`__pycache__` directories and `.pyc` files inside
Python cache directories are excluded. Similarly named source paths and standalone
bytecode files remain part of the diff.

Use `--workloads array` to replay a single workload. Use an unused output directory
(or the directory just populated by the build driver).
One fresh child reads one JSON request and emits one JSON-line observation. The
saved `fixtures.json` contains complete replay requests; add `mode` and
`iterations` from `commands.jsonl` to feed one directly to a binary. Every case
checks expected outcomes before timing (cold first use checks immediately after
its single timed preparation). Complete baseline/candidate observations must
agree, including refusal codes and original diagnostic locations. Every timed
iteration counts its actual outcome. A new refusal cannot pass as throughput.

A change that intends to alter a workload's outcome states the new outcome in
that workload's expectation. The build driver saves the merge base's own fixture
rows as `baseline-fixtures.json` and records its SHA-256 in
`instrument-pair.json`; the measuring step reads both from its output directory
and refuses a saved set the build did not record. Where the two expectations
differ, both binaries still run the candidate's workload, but each is checked once
against its own side's expectation. The row is reported as **outcome changed by
this change**, with both expectations, in `comparison.json` and the summary, and
makes no timing or memory decision; either side missing its own expectation is
an error. Every other row is compared as above: an observation that differs where
the expectation did not change remains an error, even when both sides meet that
expectation, so a changed outcome must be declared to pass. A workload in only
one fixture set is reported, not compared; a candidate-only workload is still
checked against the candidate's expectation. Without a saved merge-base set (a
new output directory or a merge base without the harness), every workload uses
the candidate's expectations. To replay with the build's view, copy
`instrument-pair.json` and `baseline-fixtures.json` into the new output directory.
The `reference-fan-in` fixture sends 100 distinct referring resources to one
supplied carrier with 5,000 unused definitions. Its `context` mode measures
public contract preparation; a counted unit test also requires one carrier
preparation and the unchanged traversal cancellation checkpoints.
The `reference-text-1000` and `reference-text-5000` fixtures repeat long reference
spellings to qualify preparation heap. The `unevaluated-patterns` fixture sends 100
`unevaluatedProperties` walks through references to five recursive `patternProperties`
subschemas; its `eval` retained heap shows any copy of those subschemas a walk keeps.
The diagnostic allocator regression also bounds cache storage growth when URI
suffixes grow from 256 to 1,024 bytes.
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

`document` validates the fixture bytes into a report and acquires the conformant
proof when available, including destruction on every operation. `read` retains
the proof outside timing and walks all operations, reading each key, alias,
description, tag and input/output presence. It counts fields and UTF-8 text
bytes, checked against independent fixture expectations before timing.
`explain` retains the prepared diagnostic contract and parsed failing value,
then requests full available diagnostics per iteration; serialization stays
outside timing. Its precheck includes reported problem count, first problem
code and original instance/schema locations, as well as the full observation.
The shim calls `PreparedContract::explain_with_control` per iteration.

Each request selects its mode's operation once, before timing. Every mode's
warmups and timed loop form a separate, never-inlined instance of one generic
function (cold first use has its own), so a timed iteration runs only that
operation, its `black_box` boundaries and outcome counting, never an engine or
mode comparison. Editing one mode cannot change another mode's instructions.
It can still move them: each loop starts where earlier code ends, SDK changes
move code the same way, and caches and branch predictors respond to alignment.
Layout effects cannot be removed entirely, which is one reason timing stays
advisory on shared runners. A timing change confined to modes that cannot reach
the edited code indicates placement, not cost. The `src/sdk.rs` wrappers keep
`#[inline(always)]`: making them `#[inline(never)]` added about 1 ns per call (2%
of scalar evaluation) without removing that placement coupling.

Deterministic document fixtures contain 1, 50 and 1,000 operations. The largest
shares a schema's `$defs` through references; all have aliases, escaped Unicode
descriptions, tags and varying contract presence. A non-conformant fixture has
six findings across shape, name, alias and schema rules. Explain fixtures have
one and 64 independent type violations. The current evaluator returns only the
first problem, so both expect **one reported problem**, with incomplete
diagnostics. The many-violation case does not measure producing a 64-problem
list. Supporting that list requires a separate evaluator change and revised
expectations.

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
flags fail the job. The workflow pins glibc's `MALLOC_MMAP_THRESHOLD_` and
`MALLOC_TRIM_THRESHOLD_` for both sides, so allocation history before timing
cannot change how large blocks are served (see docs/performance-boundaries.md).
Timing decisions and RSS remain advisory, including timing
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
