# Performance and allocation boundaries

The document-validation, regex-budget and diagnostic-allocation tables retain
measurements of engine revision
`a2e5a380018754bf8b96ffebadd4545fec0a07cb`, with documentation and measurement
harness changes only. The host was macOS 26.6.2 arm64, Rust 1.99.0. Wasm used
wasm-bindgen 0.2.129. Timings are examples, not latency guarantees or CI thresholds.
Prepared-contract and Wasm measurements were updated on 2026-10-15 below.
Use the locked dependencies and release profile to reproduce them.

## Prepared value contracts

Retain a `PreparedValueContract` for repeated validation. Reusing a conformant
document and `ResourceSet` also saves work, but still prepares a fresh contract on
each `prepare` call. The SDK does not transparently cache arbitrary custom
evaluator results.

On 2026-10-15, paired release measurements against revision `3fd152c` on macOS
arm64 / Rust 1.99.0 produced the following medians, including shared carrier inspection
and compact reference routing. Preparation includes schema parsing, minimal document proof, resource
contexts, compilation and destruction; evaluation retains the contract and parsed
value. Twelve adjacent randomized pairs used adaptive batches of at least 80 ms
targeting 120 ms. Ratio intervals are 95% paired bootstrap intervals within this
session, not portable budgets.

| Workload | Preparation before → after µs | Evaluation before → after µs | Evaluation ratio (95% interval) |
| --- | ---: | ---: | --- |
| scalar | 377.25 → 107.01 | 0.556 → 0.082 | 0.147 (0.146–0.147) |
| object | 441.32 → 161.63 | 0.796 → 0.555 | 0.698 (0.694–0.700) |
| array | 384.87 → 114.68 | 5,614.816 → 387.855 | 0.069 (0.069–0.069) |
| refs | 466.63 → 182.23 | 14.929 → 9.986 | 0.669 (0.666–0.673) |
| large-object | 385.99 → 113.34 | 7,525.669 → 988.652 | 0.132 (0.131–0.132) |

The direct SDK/jsonschema-rs 0.58.6 geometric mean over the original five
workloads × four modes (prepare, evaluate, parse, full) fell from **12.50× to
4.84×**. The peer supplies a two-outcome engine-cost reference with different
number and resource semantics. This aggregate is not the earlier 13.1× comparison
against the faster of two peers. Exact decimal-array evaluation changed by
+1.7% (ratio interval 1.007–1.026); integer shortcuts retain its general
arithmetic costs.

On 2026-10-16, preparing contracts from the operation instead of a retained
context was compared with revision `37bfd7f` by the benchmark harness's cross-API
pairs (macOS arm64, Rust 1.99.0, two batches of 20 adjacent randomized pairs).
Preparation no longer assembles a core resolution space unless an evaluator asks
for the core projection, so one-shot preparation (`compile`) fell by 1–20% and
`full` by up to 20%; repeated preparation with the same document and resources
(`context`) stayed within 0.992–1.008. Allocation counts per operation fell for
one-shot preparation and were unchanged elsewhere. The evaluation path's machine
code is byte-identical apart from addresses; sub-microsecond evaluation rows
measured up to 4% slower in these builds, which is not yet attributed.

On 2026-10-17, separating `const` and `enum` literals from the projection, a
work allowance that grows with the value and a default `pattern_depth` of 32
were compared with revision `fa2d0b9` by the benchmark harness's identical
instrument pairs (macOS arm64, Rust 1.99.0, two batches of 20 adjacent
randomized pairs, shared host). Literals are now parsed once, into an arena that
holds only them, so `large-literal` preparation fell 1.5–2% and a 128-branch
`oneOf` of constants retains 2,533,394 instead of 2,636,468 bytes. A contract no
longer keeps the annotations of the schemas its literals came from: an `enum`
beside an 8 MiB `description` retains 2.4 KB instead of 8.4 MB. Requested heap
and allocation counts were within policy on every row. Reference-chain evaluation
rows (`refs`, `reference-fan-in`, `reference-text-*`) measured 2–16% slower in
these builds, with instruction-identical hot functions at other alignments; with
64-byte function alignment for both builds the same rows measured 0.97–0.98.
`full` rows of 20,000-item arrays measured about 1–2% slower while their
preparation, parse and evaluation rows measured 0.99–1.00.

The default work allowance is 2,000,000 units per pass for values of up to
65,536 nodes and grows in proportion above that. Ordinary schemas used about
three units per value node (an array of records, an object of integer members,
an integer array and referenced records measured 2.77–3.08), so values up to the
default 1,000,000-node admission limit are decided. Examples that previously
returned `evaluation-work-limit`: 40,000 records (7.8 MB) satisfy in 66 ms;
340,000 and 499,999 members satisfy in 27 ms and 39 ms; 999,999 integers satisfy
in 24 ms. Evaluation depth is not scaled: the walk recurses, using about
0.75 KiB of stack per level of a recursive value in release builds and 2.9 KiB
in debug builds for the benchmark's `deep` schema, so the admitted 10,000 levels
would not fit an 8 MiB main-thread stack. Its stop is reported as
`evaluation-depth-limit`.

Patterns nested to the default depth of 32 prepare, and values are checked
against them, within a 256 KiB thread stack in release builds. Measured on
2026-10-17 with Rust 1.99.0, the smallest stack on which a cold preparation
completes at every depth up to 32 is 52 KiB on macOS arm64 and at most 128 KiB
on the Windows x64 runner, which reserves thread stacks in 64 KiB units; checks
need at most 16 KiB and 64 KiB. The regular-expression parser recurses at each
group level, using about 1.3 KiB per level in release builds (1.4 KiB on
Windows) and 5 KiB in debug builds, and the matcher recurses at each lookaround,
about 0.5 KiB per level in release builds. Debug builds are dominated instead by
the matcher's unoptimized frames of about 90 to 100 KiB. The meta-schema check
of every projected schema runs a pattern (the 2020-12 `$id` rule) through them,
so debug preparation needs 212 KiB on macOS whatever the pattern (228 KiB for
32 nested lookarounds) and between 256 and 320 KiB on Windows, where the
pattern stack test is therefore verified in release builds. Each nested
lookaround adds one such frame while a value is checked: two need 276 KiB in a
macOS debug build.

A fan-in workload has 100 distinct referring resources pointing to one supplied
integer schema with 5,000 unused definitions. Twelve adjacent fresh-child pairs
measure public preparation alone, after the proof and resources are built:

| Referrers | Public preparation before → after ms | Ratio (95% interval) |
| --- | ---: | --- |
| 1 | 6.333 → 6.032 | 0.960 (0.889–0.997) |
| 10 | 6.205 → 5.962 | 0.961 (0.950–0.971) |
| 100 | 8.008 → 7.739 | 0.953 (0.945–0.968) |

Repeated references with long spellings also retain only compact source handles.
The original spellings stay in the source document, and initialized carrier query
batches are released. The following fresh-child pairs isolate public preparation;
peak requested System heap includes the already-retained proof, resources and
process caches. Each case has one supplied integer target and repeats its pointer reference.
URI suffix widths are 256 and 1,024 bytes respectively. Twelve pairs give identical
peak counts (degenerate heap ratio intervals).

| References | Preparation before → after ms | Time ratio (95% interval) | Peak heap before → after MiB | Heap ratio |
| --- | ---: | --- | ---: | ---: |
| 1,000 | 11.351 → 10.727 | 0.939 (0.938–0.950) | 10.30 → 10.15 | 0.986 |
| 5,000 | 76.891 → 66.797 | 0.882 (0.860–0.894) | 55.01 → 50.38 | 0.916 |

The allocation regression widens these URI suffixes while retaining the space.
It observes zero extra retained cache bytes for both reference counts and limits
retained growth to 16 KiB and peak growth to 64 KiB; timings do not decide the test.

Reference associations are indexed once per schema space. A requested reference
prepares its target carrier once, sharing the copy and library preparation across
all referring resources. Other target carriers remain lazy. A counted regression
test requires one preparation for this workload and preserves the parent's 607
cancellation checks for projection; it does not assert wall-clock timing.

Requested System heap was measured in twelve fresh-child pairs separately from
time. Peaks below include protocol, retained owners and process caches; they
are not incremental evaluation scratch or RSS. Identical repeated counts give
degenerate intervals. Cached standard indexes add roughly 30 KiB to retained
process memory. The earlier six array/object parse/evaluate/full RSS pairs were
effectively unchanged (SDK/jsonschema-rs geometric mean 2.065× before and 2.069×
after); that RSS aggregate was not remeasured for these reference-cache corrections.

| Workload | Preparation peak before → after KiB | Evaluation peak before → after KiB |
| --- | ---: | ---: |
| scalar | 2,268.7 → 2,271.0 | 2,168.7 → 2,198.2 |
| object | 2,274.9 → 2,304.2 | 2,182.0 → 2,211.6 |
| array | 2,503.5 → 2,511.2 | 5,326.3 → 5,355.8 |
| refs | 2,298.8 → 2,330.0 | 2,284.1 → 2,313.6 |
| large-object | 4,100.9 → 4,108.0 | 18,186.1 → 18,215.7 |
| unique-array | 2,502.4 → 2,503.6 | 7,511.6 → 7,093.1 |

Exact integer shortcuts only apply where the original tokens prove the result;
all other tokens retain decimal arithmetic. Lazy reference inspection still uses
the library resolver and the complete explicit-resource identity policy. Standard
indexes cache only pinned immutable documents. Uniqueness verifies exact equality
for every hash collision. Work, depth, regex and cancellation allowances and
original-source diagnostics remain in force.

The separate [benchmark harness](../tools/evaluator-bench/README.md) and
[performance workflow](../.github/workflows/performance.yml) record outcomes before
timing and keep allocation instrumentation separate. The fan-in fixture includes
cold, compile, repeated preparation, evaluation and full modes. The two
reference-text fixtures also qualify preparation heap. Outcome checks, requested
heap and allocation counts enforce now: failed checks or a heap/count flag fail
the job. Heap flags require both >10% and >128 KiB for retained baseline, total
peak, incremental peak or requested bytes/op; count flags require both >5% and
>32/op.

A change that intends to alter a workload's outcome declares it in that
workload's fixture expectation. The comparison then checks the merge-base binary
against the merge base's expectation and the candidate against its own, reports
the row as outcome changed by this change with both expectations, and leaves it
out of timing and memory decisions. Either side missing its own expectation, or
any observation difference on a row whose expectation did not change, still fails
the job. Workloads present in only one fixture set are reported, not compared.

The harness isolates SDK calls in `tools/evaluator-bench/src/sdk.rs`. It also
measures document validation directly from bytes (including the conformant
proof), conformant operation/text reads and full available failure diagnostics.
Document scales are 1, 50 and 1,000 operations, with shared `$defs` references in
the largest, plus a document with six findings. Explain inputs contain one and
64 independent violations; today's evaluator reports only the first problem.
Checks cover that count and its original code and instance/schema locations.
The many-violation input does not qualify the cost of returning many problems.

Each mode's operation is selected before timing and runs in its own non-inlined
timed loop, so a change cannot recompile the loop of a mode it does not reach,
and no timed iteration compares mode names. Code placement still couples modes:
growing one loop moves every later one, and SDK changes move code the same way.
Layout effects cannot be removed entirely, so timing stays advisory on shared
runners.

On Linux, glibc adapts its mmap and trim thresholds to a process's earlier frees,
so a candidate whose preparation history differs from the baseline's can have
large blocks served by fresh mappings on one side and by reused heap on the other.
That once measured a 50,000-member parse at 1.9x with identical allocations and
RSS, and at 0.997 once the thresholds were pinned. The workflow therefore pins
`MALLOC_MMAP_THRESHOLD_` (32 MiB) and `MALLOC_TRIM_THRESHOLD_` (128 MiB) for both
sides; local runs on other allocators are unaffected.

On 2026-10-17 (macOS arm64, Rust 1.99.0, paired runs of 40 pairs per row on a
loaded host), removing per-iteration dispatch lowered the `baseline` floor from
5.1 to 2.9 ns and small evaluations by 1.3 to 3.9 ns; larger rows kept a median
ratio of 1.000. One extra `black_box` in the read loop moved every later loop by
120 bytes. The other non-cold rows then moved by a median of 0.17% (90th
percentile 0.57%), against 0.10% (0.48%) for two identical binaries; `one-of`
evaluation moved 1.7% and 1.9% in two runs. The previous single-function
structure, which this edit grew by exactly 64 bytes, moved by 0.14% (0.40%).
The `reference-text` evaluations run at two or three distinct speeds per
process and moved by up to 14% between differently laid out binaries in either
structure.

The workflow first tries the candidate's identical instrument against both SDK
revisions. If the baseline fails to build, it may build each revision's own
instrument only when their complete instrument diff changes `src/sdk.rs` and
SDK-owned workspace package blocks in the instrument's `Cargo.lock` exclusively.
Any fixture, timing, check, allocator, manifest, third-party lock or other
instrument change prohibits that fallback. Such pairs remain not comparable
and pass with a report. Authorized pairs are marked `cross-API`, retain the
instrument diff and SHA-256 plus lock hashes, and face the same outcome, heap,
allocation-count and timing policy as identical-instrument pairs. Every build
uses its own new target directory, because Cargo can judge archived sources fresh
and reuse another tree's binary from a shared one. Each binary embeds digests of
the instrument and SDK sources it was compiled from; a mismatch with the claimed
trees fails the job, and the pair record keeps each build's digests and binary
hash.

Timing remains advisory until about 20 calibration runs show stable noise and
enforcement is explicitly reviewed. Shared runners report a confirmed >20%
paired regression above the absolute floor and a lower 95% bound >1.20;
dedicated runners use >10% / >1.10. Timing regressions, inconclusive timing and
RSS verification flags do not fail the job. Noise requests a rerun. See the
harness for exact floors, confirmation and calibration policy.

## Preparation work bound

Preparation compiles the projected schemas with the JSON Schema library, and some
library behaviors can make that compilation grow faster than the schema. Each
time it defers a nested `$ref` target it copies every in-progress reference
placeholder, which is quadratic in a reference chain's length. It also keys its
compiled schemas by dynamic scope, so a schema reached along several paths through
resources (supplied resources or `$id` resources) would compile once per path,
exponentially in the number of branching and rejoining levels, and each of those
compilations copies the schema's own `required` names and patterns again. Only
resolution through a `$dynamicAnchor` reads the dynamic scope, so when the projected
contract declares none, the default evaluator tells the library, which then keys
by location and compiles each schema once. A contract that reaches a dynamic
anchor, for example by referencing the 2020-12 meta-schema, keeps one compilation
per path.

The library charges what a build does, in units of about a nanosecond of
compilation time or 0.7 bytes of compilation memory, whichever the work costs more
of, in the reference release build:

| Work | Units |
| --- | ---: |
| A compiled schema node, or the node built for a `dependentRequired` entry | 830 |
| Each byte of that node's location | 8 |
| A `$ref` or `$dynamicRef` resolved and entered | 750 |
| Each dynamic-scope entry keying a node's caches | 30 |
| Each dynamic-scope entry searched for a dynamic anchor | 440 |
| Each placeholder copied when a `$ref` target is deferred | 10 |
| Each property name copied out of `required` or `dependentRequired` | 110 |
| Each byte of such a name | 2 |
| Each byte of a `pattern` | 1 |

The weights were fitted to 126 measured builds in 20 classes of contract shape.
Where time dominates, compile time is within about 25% of a nanosecond per unit;
where copying dominates (long `required`, `dependentRequired` and `pattern` values
compiled once per scope), compile memory is 0.7 to 1 byte per unit and time less.
The charge counts work and never measures it, so a build's charge is the same on
every platform.

The default evaluator allows 250,000,000 units plus 4,096 per projected
definition, that is, per schema position the contract reaches, named or not, and
refuses beyond that with `LimitExceeded` (`preparation-work-limit`). Compiling an
ordinary definition once costs about 2,700 units, so the number of definitions
alone does not refuse a contract; the floor bounds what grows beyond them. Builds
of the review corpora spend under 1% of their allowance, except a 3,000-definition
contract (3%) and a layered model holding a schema-valued property (11%); layered
models across resources without one spend under 2%.

Release measurements on macOS arm64, Rust 1.99.0, 2026-10-18, against `ba5ca95`
(minimum of three preparations; peak resident memory of the whole process; the
machine was shared, so times run high):

| Shape | Before | After |
| --- | --- | --- |
| 7 layers of 6 object schemas, each referencing 3 of the next, as 42 supplied resources | prepared, 25.6 ms | prepared, 3.2 ms |
| The same model as `$id` resources in one document | prepared, 25.6 ms | prepared, 3.2 ms |
| 12 layers of 6 schemas, each referencing 3 of the next, supplied | prepared, 10.3 s, 3.9 GB | prepared, 5.4 ms, 15 MB |
| 24 levels of resource diamonds | stopped after 300 s at 17 GB | prepared, 1.9 ms |
| The 7-layer model with one property holding a 2020-12 schema | prepared, 25.5 ms | prepared, 25.5 ms |
| 16,000-link same-document chain | prepared, 238 ms | prepared, 235 ms |
| 49,000-link same-document chain (1.79 MB) | prepared, 1.7 s, 247 MB | refused, 0.71 s, 232 MB |
| 13 levels of resource diamonds with a dynamic anchor | prepared, 278 ms | refused, 296 ms |
| 1,000 `$id` resources sharing one with `required` of 100,000 names, with a schema-valued property | prepared, 4.6 s, 10.5 GB | refused, 0.24 s, 304 MB |
| The same without the schema-valued property | prepared, 4.0 s, 10.5 GB | prepared, 69 ms, 94 MB |

Generated contracts whose `unevaluatedProperties` and `unevaluatedItems` walks
cross resources took from 1.4 s to more than 65 s each to prepare on `4b6afd3`
(one preparation each, 25 contracts of up to 23 resources); they now prepare in 2
to 26 ms, with the same verdicts where `4b6afd3` finished. One of 4 supplied and 6 contained
resources (8.2 KB) took 62 s and prepares in 3.7 ms. With a property holding a
2020-12 schema, each of the 25 refuses in 0.29 to 0.37 s.

Same-document chains refuse from 23,497 links. With a reachable dynamic anchor,
resource diamonds refuse from 13 levels, and a 20,000-name `required` list shared by
more than 103 resource paths refuses. A refused build is released link by link, as
a prepared one is, so refusing a long chain needs no more stack than preparing it.
Evaluation still follows every path of a diamond, within the evaluation limits.
A schema compiled once is reached from its later paths through a pending node, as
the library already does within one resource, which can cost evaluation steps. In
an adversarial corpus of 15,400 values, 647 needed more steps than before, at most
32% more; at budgets of 10 to 400 steps, 45 value and budget pairs lost their
verdict, 9 gained one, and 26 explanations kept their failure but no longer listed
its problem. A value that needs nearly its whole `evaluation_steps` budget can
therefore return no verdict, or an explanation without its problem; a verdict that
is reached never changes.
Outside the charge stay the SDK's linear projection, pattern admission and each
distinct pattern's one-time regex build, and registry setup, which grows
quadratically with the number of projected resources (55 ms for 2,000 supplied
resources, 0.47 s for an 8,000-resource `$id` chain); a refused 49,000-link chain
spends about a third of its time there and in projection. `Limits::schema_resources`
refuses a contract reaching more than a configured number of resources before
that setup; there is no cap by default.

## Document validation

Run from the repository root with no concurrent compilation:

```sh
cargo test --release --locked -p openbindings --test indexed_lookup_measurement -- --ignored --nocapture
```

Each fixture has two warmups and five samples; the table reports the median.
Parsing and full document validation are timed. Fixture generation, result
destruction and assertions are outside the timer. Duplicate fixtures contain N
distinct names followed by the same N names. Reference fixtures contain N schemas
that each reference the final schema.

| Fixture | N | Input bytes | Median ms |
| --- | ---: | ---: | ---: |
| Duplicate members | 10,000 | 197,828 | 4.019 |
| Duplicate members | 20,000 | 417,828 | 5.494 |
| Duplicate members | 46,000 | 989,828 | 11.153 |
| Schema references | 10,000 | 348,942 | 11.691 |
| Schema references | 25,000 | 913,942 | 29.480 |
| Schema references | 40,000 | 1,468,942 | 47.743 |

Duplicate detection and object lookup use indexed decoded names. Ordinary hashes
produce work proportional to source/key size; deliberate collisions can require
more exact comparisons. Caller byte and node limits still bound admission. These
fixtures do not characterize arbitrary schema evaluation or cold compilation.

## Regex budget

```sh
cargo run --release --locked -p openbindings-json-schema-evaluator --example regex_budget
```

The example prepares `[A-Z]+[0-9]+` once and checks strings containing only `a`.
Preparation and value construction are outside the timer. After two warmups it
reports five-sample medians with the default 2,000,000-step regex allowance:

| String bytes | Result | Median ms |
| ---: | --- | ---: |
| 1,024 | fails | 0.014 |
| 65,536 | fails | 0.806 |
| 1,048,576 | no-verdict, `regex-work-limit` | 5.370 |

An unanchored search can exhaust the allowance on a large ordinary string.
Exhaustion is `LimitExceeded`, not a failed value contract or semantic
undefinedness. `Limits::regex_steps` changes the allowance, not a wall-clock
deadline. Anchor a pattern only when anchoring expresses the intended contract.

## Wasm size

Build the bundled asset and measure its exact and compressed byte lengths:

```sh
npm --prefix packages/typescript run build:wasm
python3 - <<'PY'
from pathlib import Path
import gzip, hashlib
b = Path('packages/typescript/src/wasm/openbindings_wasm_bg.wasm').read_bytes()
print('bytes', len(b), 'gzip-9', len(gzip.compress(b, compresslevel=9, mtime=0)))
print('sha256', hashlib.sha256(b).hexdigest())
PY
```

The 2026-10-15 implementation asset is **6,062,372 bytes**, or **1,658,935 bytes** with gzip level 9.
Its SHA-256 is `14148399cb8a465992af93ad6c19aa1414c7f445f6eceb82c689d3ee6ab73fbc`.
The build uses the workspace release profile and removes the Wasm name section.
These lengths exclude JavaScript, declarations, runtime memory and host overhead.
`build-info.json` beside the generated asset records its source and tool identities.

## Allocation bounds

```sh
python3 tools/verify-diagnostics.py
```

The probe checks 69 large generated documents and 300 admitted corpus documents,
with three samples each; five corpus parse refusals remain recorded. Measurements
are incremental live System allocations. Parsing and warmed compilation are
excluded; complete validation uses fresh document owners. This measures neither RSS
nor Wasm capacity. Outputs are under `target/document-diagnostics`.

| Region | Observed maximum | Fixture-specific regression threshold |
| --- | ---: | --- |
| Fixed-schema validity | 17,573,888 bytes | 18 MiB |
| Fixed checks above that input's validity peak | 8,472,586 bytes | 10 MiB |
| Full validation above that input's validity peak | 8,797,823 bytes | 12 MiB |
| Prepared cheap value checks | 113 bytes | 65,536 bytes |

The absolute full-validation peak was 17,949,677 bytes. Value cases exercise
1,000- and 50,000-element arrays, including partial evaluation. These are bounds
for the recorded fixtures, not all inputs. Document findings separately retain at
most 4,096 entries and 8 MiB of pointers. Value explanations have library
allocation and stack limits and may overflow on deep inputs. See
[allocation methodology](../tools/diagnostic-memory-probe/README.md) and
[MSRV and stack qualifications](support.md).
