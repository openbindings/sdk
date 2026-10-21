# Performance and allocation boundaries

The document-validation and diagnostic-allocation tables retain measurements of
engine revision `a2e5a380018754bf8b96ffebadd4545fec0a07cb`, with documentation
and measurement harness changes only. The host was macOS 26.6.2 arm64, Rust
1.99.0. Wasm used wasm-bindgen 0.2.129. Timings are examples, not latency
guarantees or CI thresholds. Prepared-contract measurements were updated on
2026-10-15 and, with the preparation and pattern sections and the Wasm size, on
2026-10-20 below; the compiled-pattern memory and the exponential evaluation
example were measured on 2026-10-21. Use the locked dependencies and release
profile to reproduce them.

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

On 2026-10-20, composing the default evaluator over the unmodified library was
compared with `94b6991`, where core already used the unmodified library, by the
benchmark harness's cross-API pairs (macOS arm64, Rust 1.99.0, two batches of 20
adjacent randomized pairs). Over the 85 compared rows the geometric mean time
ratio was 0.88: first use 0.75 to 0.91, preparation 0.89 to 1.17 (geometric
mean 0.96) and evaluation 0.27 to 1.48 (geometric mean 0.78); document rows
were unchanged. Eleven rows declare a changed outcome (a missing resource's
no-verdict now names it, an absent pointer target refuses preparation instead
of being undefined, a megabyte string fails instead of exhausting the former
regex allowance, explanations without schema details) and make no timing or
memory decision. The `unevaluated-patterns` preparation rows exceed the heap
and allocation policy: they take 1.16 to 1.18 times as long (2.57 ms to
2.99 ms), with 9 to 10% more allocations, 11 to 13% more requested bytes and
18 to 20% more transient peak heap (323 KB), while the prepared contract
retains 12% less. That residual is the cost of the unevaluated-walk transform
on this shape, where the library's walk compiles the `true` subschemas the
transform leaves once for each walk; the harness declares it accepted, with
bounds just above these ratios, and the enforced comparison passes. The
workload's evaluation row, which the stack guard covers, measured 1.00.
`work-stop`, evaluating under a 16-unit cap, stops 71 ns later. Process heap
retained with a prepared contract before any timed operation fell by about
458 KB.

No evaluation work cap applies by default, so values as large as the default
1,000,000-node admission limit are decided, in time that grows with the schema
and the value. A cap set with `Limits::evaluation_steps` charges a unit for each
schema a reference enters and for each element, member and literal comparison
step, and stops a pass with `evaluation-work-limit` once it spends the cap; a
700,000-integer array against a referenced item schema spends two units per
element (`evaluation_budgets`). Preparing with a cap places a marker keyword
before every reference, so a capped contract's validator is slightly larger.

Evaluation depth is bounded before evaluation. Preparation derives from the
bundled contract's references a bound on the schema objects an evaluation path
can hold for a value of a given depth, and a value whose bound exceeds
`evaluation_depth` (1,024) gets `evaluation-depth-limit` without being
evaluated. The bound is the lesser of two: one from the reference graph's
strongly connected components, and one from its descents (a path holds at most
the entry's same-instance run, then for each level of the value one descent
and one same-instance run), so a long chain of descents admits shallow values.
The check reads the value's node count, scans its nesting only when the count
does not settle it, stops at the first container nested too deep, and
allocates nothing for a value nested at most 32 levels. The bound is static,
so a deep contract whose paths only some values follow refuses the others too
(a same-instance chain of 400 `anyOf` levels refuses every value).

The library's `unevaluatedProperties` and `unevaluatedItems` validators take
larger frames and recurse through the schemas they read, about 1 KiB per
level for one recursing through its own subschema beside an `anyOf` and up to
about 18 KiB per level for one whose walk follows a 30-reference chain,
against about 330 bytes for `additionalProperties`; the static bound does not
model that. So a contract holding one is guarded as it evaluates: every read
the library makes of a child instance already passes through the SDK's value
adapter, which compares the stack address it runs at with the address the
pass started at (each level of the value is such a read). A pass that has
used more than 128 bytes per unit of `evaluation_depth`, 128 KiB at the
default, stops with `evaluation-depth-limit`: from then on every read fails,
so the library stops descending and its answer is discarded. The guard adds
no schema keyword, so preparation is unchanged; each read pays an address
comparison. Checking a 20,000-integer array took 131 µs instead of 100 µs, and
a 100-level object 9.8 µs instead of 9.5 µs, against a contract holding an
unevaluated keyword that does not apply to the value, compared with the same
contract without it. At the default limit, release builds prepare 39
recursive contract shapes, including both reviews' unevaluated chains, and
check and explain their deepest admitted values on a 256 KiB thread
(`evaluation_stack`); the deepest admitted range from 6 levels, for a
30-reference walk at every level, to 340. Building an explanation takes about
twice a check's stack per level, so explanations are built only within half
the limit.

Evaluation time has no default bound. It grows with the schema and the value,
and for some schemas exponentially: 30 levels of `anyOf` that each name the
next level twice, ending in `false` (under 2 KB), take about 2.5 s to check
`{}`, 28 levels 1.1 s and 26 levels 0.29 s (measured 2026-10-21 on revision
`2b45ce5`). `evaluation_steps` bounds it; a cap of 1,000,000 stops the
30-level check with `evaluation-work-limit` in about 20 ms. The same nest of
`unevaluatedProperties` holders no longer reaches evaluation: from 8 levels its
annotation walk weighs more than the evaluator admits and preparation refuses
it (`unevaluated-walk-limit`) in under 1 ms, and 7 levels prepare and check
`{}` in about 4 ms.

Patterns nested to the default depth of 32 prepare, and values are checked
against them, within a 256 KiB thread stack in release builds. The regex
crate's parser and compiler recurse at each group level; a debug build needs up
to about 1 MiB to compile 32 nested quantified groups, so the pattern stack test
runs in release builds. A `pattern` keyword's expression is compiled the first
time a string reaches it, so preparation compiles only `patternProperties`
names. Each pattern's form compiles within what the contract's pattern memory
gives it (below), at most 32 MiB, enough for counted repetitions of large sets
(`^.{1,10000}$` compiles in about 40 ms with a 49 MB process peak), and a form
longer than `pattern_bytes` is neither
written out nor compiled: a 1 MiB pattern of 524,287 `\S`, whose form would
be about 115 MB, gives `pattern-size-limit` with a 100 MB process peak.

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
for every hash collision. Depth and cancellation limits and original-source
diagnostics remain in force.

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

A regression that was reviewed and accepted is declared in the harness's
`accepted-residuals.json`, one exact workload, mode and metric per entry, with
the largest ratio accepted and the decision that accepted it. A declared flag
within its bound passes and is reported first in the summary; beyond its bound,
or undeclared, a flag still fails the job, and an entry whose row no longer
regresses is reported as stale.

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
instrument only when their complete instrument diff changes `src/sdk.rs`,
SDK-owned workspace package blocks in the instrument's `Cargo.lock` and fixture
expectations exclusively; a fixture edit qualifies only when both generators
produce identical requests. Any fixture request, timing, check, allocator,
manifest, third-party lock or other instrument change prohibits that fallback.
Such pairs remain not comparable and pass with a report. Authorized pairs are marked `cross-API`, retain the
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

## Preparation

Preparation projects the contract, bundles its definitions into one resource and
compiles that resource with the library once, so every reached schema compiles
once, whatever paths reach it. A schema reached through a `$dynamicRef` compiles
once for each dynamic context it is reached in; a contract that would specialize
more than 100,000 schemas, add specialized copies holding more than 16,777,216
units of JSON beyond the projected program (one for each JSON value and one for
each byte of its strings and member names, so a 4,096-name `required` copied
into 8,192 contexts counts as the 470 MB it is), or hold more than 4,000,000
dynamic-scope entries across its distinct contexts refuses with
`program-specialization-limit`. These count the SDK's own output; the library's
compiled program for an admitted contract can be larger. A context holds only
the anchor names some `$dynamicRef` looks up, and only those it assigns.
Preparation has no work charge of its own. Core's projection refuses a contract
reaching more than 100,000 schema positions (`schema-node-limit`), which bounds
its time and memory. Within that bound, two library behaviors grow faster than
the contract. Each time the library defers a nested `$ref` target it copies
every in-progress reference placeholder, so a long same-document reference chain
prepares in time quadratic in its length. An `unevaluatedProperties` keyword
walks the schemas it can reach once for each schema holding it, and compiles
that walk by recursion, so preparation weighs each walk and refuses one that
weighs more than 105 (`unevaluated-walk-limit`, at the keyword). A reference
weighs 4, each `allOf`, `anyOf` or `oneOf` on its path 5 more, each `if`,
`then`, `else` or `dependentSchemas` 7 more, and reaching another holder 1 more,
since the library compiles that holder's walk there, so each level of a walk
weighs at least the KiB of stack it takes on every host measured (a `$ref` takes
at most 3.8 KiB, an `allOf` level 12.2 KiB and a `then` or `dependentSchemas`
level 13.3 KiB). That admits 26 direct `$ref`s (21 when every level also holds
`unevaluatedProperties`), 8 levels of `allOf: [{"$ref": ...}]` inheritance (7),
and 7 levels through `then`, `else` or `dependentSchemas` (6). The library also
compiles up to eight `$ref` targets on the stack before it defers the next, and
those it is compiling when it reaches the holder add up to 48 KiB. `walk_stack`
prepares the longest walk of each kind below 0 to 40 levels of enclosing
`properties` on a 256 KiB thread with a fifth of it already in use, so every
host keeps at least that much headroom. Release builds with Rust 1.99.0 on
GitHub's hosted runners, 2026-10-21: the most stack that can already be in use
on a 256 KiB thread when the longest admitted walk still prepares at its worst
enclosing depth, giving each row's kind with the least to spare (the thread
itself leaves 250 KiB usable on Linux, 226 KiB on Windows and 255.5 KiB on
macOS):

| Walk | Levels admitted | Linux x86_64 | Windows x86_64 | macOS arm64 |
| --- | --- | --- | --- | --- |
| `$ref` | 26 | 91.5 KiB, 36% | 58.0 KiB, 23% | 107.5 KiB, 42% |
| `allOf`, `anyOf` or `oneOf` inheritance | 8 | 91.0 KiB, 36% | 57.0 KiB, 22% | 107.0 KiB, 42% |
| `then` or `else` | 7 | 96.5 KiB, 38% | 62.0 KiB, 24% | 112.5 KiB, 44% |
| `dependentSchemas` | 7 | 92.5 KiB, 36% | 59.0 KiB, 23% | 109.0 KiB, 43% |
| A holder at every level | 21, 7 or 6 | 102.5 KiB, 40% | 69.0 KiB, 27% | 119.0 KiB, 46% |

Subschemas written in place rather than referenced weigh less and keep more:
11 levels of `allOf` and 9 of `then` or `dependentSchemas` are admitted, and
keep at least 24% on Windows. `unevaluatedItems` walks take less stack than
`unevaluatedProperties` walks of the same kind and keep at least 37%. The
previous weighting, measured on macOS alone, admitted 16 levels of `allOf`
inheritance, which overflowed a 256 KiB thread on Linux. The stack guard bounds
validation, not this compilation.

Release measurements on macOS arm64, Rust 1.99.0, 2026-10-20, against `b6ef531`
(minimum of three preparations in one fresh process per shape; peak resident
memory of the whole process):

| Shape | Before | After |
| --- | --- | --- |
| 7 layers of 6 object schemas, each referencing 3 of the next, as 42 supplied resources | prepared, 2.9 ms, 13 MB | prepared, 2.9 ms, 12 MB |
| The same model as `$id` resources in one document | prepared, 3.9 ms, 13 MB | prepared, 3.1 ms, 12 MB |
| 12 layers of 6 schemas, each referencing 3 of the next, supplied | prepared, 5.7 ms, 16 MB | prepared, 5.3 ms, 14 MB |
| 24 levels of resource diamonds | prepared, 1.7 ms, 11 MB | prepared, 1.5 ms, 10 MB |
| The 7-layer model with one property holding a 2020-12 schema | prepared, 26.5 ms, 35 MB | prepared, 4.1 ms, 13 MB |
| 16,000-link same-document chain | prepared, 216 ms, 93 MB | prepared, 203 ms, 91 MB |
| 49,000-link same-document chain (1.79 MB) | refused, 0.64 s, 251 MB | prepared, 2.0 s, 273 MB |
| 13 levels of resource diamonds with a dynamic anchor | refused, 0.31 s, 109 MB | prepared, 0.9 ms, 10 MB |
| 1,000 `$id` resources sharing one with `required` of 100,000 names, with a reachable dynamic anchor | refused, 0.20 s, 305 MB | prepared, 68 ms, 86 MB |
| The same without the dynamic anchor | prepared, 75 ms, 88 MB | prepared, 59 ms, 103 MB |
| 1,000, 10,000 and 20,000 supplied resources | 31 ms, 1.38 s, 5.05 s | 19 ms, 0.22 s, 0.43 s |
| 10,000-resource `$id` chain | prepared, 1.20 s, 150 MB | prepared, 0.21 s, 109 MB |
| 99,990-resource `$id` chain (8 MB) | unfinished after 400 s | prepared, 8.0 s, 903 MB |
| 100 `unevaluatedProperties` walks reaching five recursive `patternProperties` subschemas | prepared, 2.7 ms | prepared, 3.0 ms |

"Refused" before is `preparation-work-limit`, the work charge the forked
compiler applied. Registering one bundled resource makes preparation linear in
the resources a contract reaches, where registering each resource separately
was quadratic. `Limits::schema_resources` still refuses a contract reaching more
than a configured number of resources before compiling it; there is no cap by
default. The last row is the benchmark's `unevaluated-patterns` workload: the
library's walk compiles the `true` the unevaluated-walk transform leaves for
each pattern, once for each of the 100 walks, where the fork's walk compiled
only the patterns.

The evaluator bounds what the SDK itself reads and writes: core's document
admission limits and its 100,000 schema positions per contract; each pattern's
bytes and translated form (`pattern_bytes`) and the forms kept for all of a
contract's patterns together (`total_pattern_bytes`, 4 MiB); the memory the
regex crate builds for all of a contract's compiled patterns together
(`total_pattern_memory`, 64 MiB, below); and the definitions (100,000),
JSON (16,777,216 units) and dynamic-scope entries (4,000,000) dynamic
specialization produces. It does not estimate the memory of the library's
compiled program, which grows with what the library compiles.
`Limits::schema_resources` is a tighter cap an application can set. Measured by
the same method:

| Shape | Result |
| --- | --- |
| 99,990-resource `$id` chain (8 MB) | prepared, 8.0 s, 903 MB |
| 99,990-link same-document chain (3.7 MB) | prepared, 9.1 s, 457 MB |
| 100,000-link same-document chain | refused (`schema-node-limit`), 0.12 s, 119 MB |
| 11 levels of two paths, one declaring that level's dynamic anchor, ending in a schema of 27 subschemas reached in 2,048 dynamic contexts | prepared, 0.71 s, 450 MB |
| A chain of 2,800 `$id` resources, each declaring and dynamically referencing its own anchor | prepared, 0.26 s, 105 MB |
| The same with 8,000 resources | refused (`program-specialization-limit`, contexts), 0.8 s, 166 MB |
| 13 levels of a two-way `anyOf` over dynamic anchors, with a 4,096-name `required` at the leaf | refused (`program-specialization-limit`, JSON), 0.6 s, 150 MB |
| 1,000 holders of `unevaluatedProperties`, each inheriting a base of 4,000 properties | prepared, 0.34 s, 400 MB |

The last row, and the 2,048-context row, are within every bound: the
memory is the library's compiled program.

`total_pattern_bytes` counts the forms the evaluator keeps, not the patterns.
A form spells every set out (`\S` takes 191 bytes), so 380 `pattern` values of
5,000 `\S` each, 3.8 MB in all, have 363 MB of forms; preparation counts each
form as it is written and refuses at the fifth. Each distinct `pattern` form is
kept once, shared by its matcher, and a schema holding it holds only its
matcher's index; each `patternProperties` name keeps its own form, which the
library matches, and counts. The diagnostic memory probe measures that
preparation, and the same document with the patterns as annotations, by
counting allocations:

| Shape | Heap added at peak, `e0338d6` | Now | Annotations instead |
| --- | --- | --- | --- |
| 380 such patterns | 1.22 GB, prepared | 23.0 MB, refused | 23.0 MB |
| 4 such patterns | 12.9 MB | 5.3 MB | 0.27 MB |
| 5 such patterns | 16.1 MB, prepared | 6.1 MB, refused | 0.32 MB |
| One such pattern 380 times | 501 MB, 2.1 s | 23.0 MB, 26 ms | 23.0 MB |
| 5 such `patternProperties` names | 151 MB, prepared | 6.1 MB, refused | 0.75 MB |

`total_pattern_memory` (64 MiB) bounds what the regex crate builds for a
contract's compiled patterns: each distinct `pattern` form the run matcher does
not match, and each `patternProperties` name, through its two documented size
settings. A quarter of it, at most 2 MiB each, is the patterns' search caches
(`dfa_size_limit`), in equal parts; a cache bounds only how fast a search runs.
Each form first compiles within 48 KiB (`size_limit`). Every compiled pattern
also holds memory neither setting counts (about 7.7 KB for a small
`patternProperties` name), so a contract compiling more than the rest holds at
48 KiB each, 1,024 at the default, refuses at the next, before compiling any.
The forms that need more share what is left: each is given an equal part,
those that need less than their part keep only the least they need, to within
a sixteenth, and the rest is shared again among the others, until all fit their
parts, none does, or the parts grow by no more than a sixteenth; those that
then do not fit are refused, and none is given more than 32 MiB. What a form is
given depends only on the contract's forms, so no outcome depends on the order
strings reach the patterns, and small patterns change a large one's outcome
only through their 48 KiB each: a counted-repetition name such as
`^.{1,10000}N$`, about 13.2 MiB, compiles beside up to 743 small patterns,
four compile together and five do not fit.

The memory is shared once per contract, the first time a form is found that
does not fit its 48 KiB: at preparation for a `patternProperties` name, when the
SDK compiles each name's form and every `pattern` form, or when a string first
reaches a `pattern` form, when it compiles those not yet compiled. The library
takes one size setting for every name, so it first compiles the names within
48 KiB; when one does not fit, its build within the first round's parts is that
round, and otherwise it builds again within the largest size any name is given.
Refusing forms that together need more than is left takes about as much
building as is left, since an attempt that does not fit stops once it passes
its size: 40 such names refuse in about 0.2 s. Dividing the whole memory
equally among all of a contract's patterns would refuse them in milliseconds,
but would refuse `^.{1,10000}$` beside ten small patterns too.

Each row ran in a fresh release process on macOS arm64 against revision
`2b45ce5` (before) and with the shared memory (now), 2026-10-21; the time is
preparation and, where it prepared, one check of a value reaching the patterns,
and the peak is the process's maximum resident set, which includes parsing and
validating the document:

| Shape | Before | Now |
| --- | --- | --- |
| 40 `patternProperties` names `^.{1,10000}N$` (909 bytes) | prepared, 1.6 s, 1.13 GB | refused (`pattern-total-memory-limit`), 0.2 s, 12 MB |
| 300 such names (6.8 KB) | not run here: about 28 MB per name, over 10 s in an earlier review | refused, 0.24 s, 11 MB |
| 4 such names | prepared, 0.16 s, 136 MB | prepared, 0.16 s, 136 MB |
| `^.{1,10000}$` alone | prepared, 0.04 s, 50 MB | prepared, 0.04 s, 49 MB |
| `^.{1,10000}$` beside 100 small names `aN\|b` | prepared, 0.04 s, 51 MB | prepared, 0.05 s, 53 MB |
| `^.{1,10000}$` beside 1,000 small names | prepared, 0.07 s, 62 MB | refused, 0.03 s, 16 MB |
| A `pattern` `^.{1,10000}0$` beside 100 small `pattern` values, checking a value that reaches it | 0.04 s, 52 MB | 0.04 s, 52 MB |
| Two names each compiling to about 29.7 MiB | prepared, 0.17 s, 170 MB | prepared, 0.18 s, 171 MB |
| 40 properties whose `pattern` is `^.{1,10000}N$`, checking a value that reaches each | 2.3 s, 1.14 GB | no verdict (`pattern-total-memory-limit`), 0.2 s, 12 MB |
| 100 names `[01]*1[01]{20}zN`, checking a 64 KiB key of random bits | 2.4 s, 341 MB | 1.0 s, 37 MB |
| 20,000 names `xNy+` (309 KB) | prepared, 0.38 s, 259 MB | refused, 0.07 s, 78 MB |

The 100-name row is the search cache: each name's automaton has a state for
every recent run of 21 bits, so each search filled its 2 MiB cache, which the
regex keeps; its part of the caches' quarter now holds it to 164 KiB. The
diagnostic memory probe bounds the 40-, 300- and 4-name and 100-name shapes by
counting allocations (the 40-name refusal adds 2.1 MB at its peak to the same
document with property names, and the 300-name one 140 KB). The JSON Schema
Test Suite, the specification's corpus, this repository's fixtures, the review
corpora and the public OpenAPI descriptions the OpenAPI client's field study
collected hold 5,389 distinct patterns the evaluator compiles (Stripe's
description holds the most of any document, 75). All but one compile within
48 KiB. The exception, `^[a-zA-Z0-9]{0,4096}$` from a peer library's tests, needs
more as a name and compiles beside up to 1,013 other patterns; as a `pattern`
value the run matcher matches it.

A host that prepares schemas from
untrusted parties should prepare them in an isolated worker or process with
an operating-system memory limit, as it would with any JSON Schema library.
Values checked against trusted schemas stay bounded by `evaluation_depth`
and, when the host sets one, `evaluation_steps`.

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

## Pattern matching

Patterns match on the regex crate's finite automata, in time linear in the
string, so there is no regex step allowance. In the benchmark, `[A-Z]+[0-9]+`
fails a 64 KiB string of `a` in 115 µs (420 µs on the former backtracking
matcher), and a 1 MiB string, which exhausted that matcher's default
2,000,000-step allowance (`regex-work-limit`), now fails in about 2 ms.
`^(a+)+$` decides strings of a million characters at the same linear cost. A
pattern that needs a backtracking engine gives no verdict instead
(`pattern-backtracking`). Anchor a pattern only when anchoring expresses the
intended contract.

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

The 2026-10-20 implementation asset is **5,795,269 bytes**, or **1,595,571 bytes**
with gzip level 9 (`94b6991`: 7,445,434 and 1,951,593). Its SHA-256 is
`37298181233039e9bdf1b15e17f06921f066e6f253348285046155ae7be6e113`.
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
1,000- and 50,000-element arrays, directly, through a reference and under a work
cap. These are bounds
for the recorded fixtures, not all inputs. Document findings separately retain at
most 4,096 entries and 8 MiB of pointers. Value explanations have the library's
allocation limits and are made only within half the evaluation depth limit. See
[allocation methodology](../tools/diagnostic-memory-probe/README.md) and
[MSRV and stack qualifications](support.md).
