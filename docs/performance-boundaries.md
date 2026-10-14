# SDK performance boundaries

The historical series below clarifies the immutable 2026-10-08 migration evidence
at `ed2bf08718e7995a06d26b1a3534c326eb8bacc7`, without a new Go/Rust comparison.
The end-to-end headline (88.13 ms Go / 19.83 ms Rust; 77.5% elapsed reduction,
about 9% higher fresh-process peak RSS) remains the comparison of equivalent
complete work on its frozen 1,000-schema fixture. Independent numeric recomputation is a separate check; this note states
boundaries from the preserved harness source.

| Series | Timed Rust work | Timed Go work | Valid interpretation |
| --- | --- | --- | --- |
| e2e | Parse, validate, new context, prepare, parse value, validate | Validate document, resolve context, compile, parse value, validate | Complete comparable caller work; headline. |
| compile | New context plus prepare on an existing document | Compile on an existing resolved context | Different setup inclusion; not an isolated compiler speed ratio. |
| reuse | Prepare again in a warmed context with caching | Repeated compilation in an existing resolved context | Public cache-policy comparison; not equal cold compilation. |
| multi | Parse document, new context, 16 preparations/validations; one `{}` value parsed outside timer | Parse document, resolve, 16 compilations/validations; `{}` parsed within each iteration | Shared caller strategy with different value parsing; not an exact equivalent isolated stage ratio. |
| parse | Raw ParsedDocument parsing | ParseDocument includes supported-version/structural-model checks | Descriptive API boundary only; not an equal structural-check comparison. |
| retained | Validate an already parsed value against a retained contract | Same retained-value strategy | Reused validation; excludes preparation and value parse. |
| empty/full | Parse value then validate against retained contract | ValidateJSON parses value then validates retained contract | Value conversion is inside both timers. |

The historical `reuse` row describes the former core preparation cache. Current
`ValueContracts::prepare` compiles on every call. Reuse now comes from retaining a
ready `ValueContract`, or an invocation `operation::PreparedOperation` handle.

Rust's context includes resource indexing/projection setup. Native `time -l` RSS
includes process setup and warmup; it is not allocated-byte traffic. Browser
initialization was in fresh contexts, not necessarily fresh browser processes;
local uncompressed transport is not an internet loading estimate. JS/Wasm and
native Rust timings do not establish a speedup over a pure TypeScript engine.

## 2026-10-13: CR-1 and CR-2

Duplicate detection hashes decoded UTF-16 units and compares only distinct names
in the same hash bucket. Equal repeats retain their first lookup slot, while all
original members and duplicate token coordinates remain in the arena. Objects
with at least 16 members retain these buckets after parsing for pointer and
evaluator lookups. Smaller objects scan at most 15 members. The retained index
stores member positions, without copying decoded names, and is immutable across
retained views and threads.

For ordinary hashes, parsing costs O(source bytes plus decoded key units), and
lookup costs O(token units plus matched key units). Distinct keys deliberately
colliding in one hash bucket require up to the bucket's distinct-name count in
exact comparisons per insertion or lookup. Each comparison is bounded by key
length; caller byte and node admission limits bound both lengths and bucket
population. RandomState uses a per-arena random key, but does not guarantee a
constant-size bucket. Array indexing and RFC 6901 decoding keep their existing
semantics. The evaluator charges the indexed table access and every candidate
comparison against its existing work budget.

The manual measurement is in
`crates/openbindings/tests/s1_release_measurement.rs`. Run it after sourcing the
program environment, with no concurrent compilation:

```sh
cargo test --release --locked -p openbindings --test s1_release_measurement -- --ignored --nocapture
```

Each fixture has two warmups and five measured assessments, with the median
reported. Input generation, result destruction and verdict assertions are outside
the timer; parsing and all document assessment work are inside. The CR-1 fixture
has N distinct keys followed by the same N keys inside `x-e`. The CR-2 fixture has
N schema entries, each referencing the last schema through `#/schemas/s{N-1}`.
These are measurements, with no timing acceptance assertion. Deterministic
regressions at N = 10,000 / 20,000 / 40,000 assert N repeated-key comparisons,
zero distinct-key comparisons, N successful indexed member probes, and 3N probes
for reading and resolving all schema references.

Native arm64, Rust 1.99.0, release profile with thin LTO, measured 2026-10-13:

| Finding | N | Source bytes | Median assessment milliseconds |
| --- | ---: | ---: | ---: |
| CR-1 | 10,000 | 197,828 | 4.248 |
| CR-1 | 20,000 | 417,828 | 9.921 |
| CR-1 | 46,000 | 989,828 | 21.533 |
| CR-2 | 10,000 | 348,942 | 26.010 |
| CR-2 | 25,000 | 913,942 | 80.616 |
| CR-2 | 40,000 | 1,468,942 | 211.586 |

These whole-assessment measurements include work outside parsing and pointer
lookup. The deterministic probe counts establish the cost of the repaired paths;
elapsed medians describe the complete caller operation on this machine.

Memory was measured on native arm64 with an external counting System allocator
around `JsonValue::parse`, using these same generated fixtures before and after
the change. The input buffer was allocated before sampling. Retained bytes are
outstanding allocation layout sizes while the value is alive; peak bytes are the
maximum outstanding sizes during parsing. This excludes allocator bookkeeping
and is not RSS. The index column is the difference in retained bytes, including
the added arena fields and outer index table. Equal repeats do not add slots.

| Finding | N | Source bytes | Before retained bytes | After retained bytes | Added index bytes | After parse peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| CR-1 | 10,000 | 197,828 | 6,227,368 | 6,768,348 | 540,980 | 6,768,536 |
| CR-1 | 20,000 | 417,828 | 12,476,680 | 13,558,332 | 1,081,652 | 13,558,520 |
| CR-1 | 46,000 | 989,828 | 25,107,304 | 27,270,300 | 2,162,996 | 27,270,488 |
| CR-2 | 10,000 | 348,942 | 6,494,192 | 7,035,172 | 540,980 | 7,035,360 |
| CR-2 | 25,000 | 913,942 | 13,524,216 | 14,605,868 | 1,081,652 | 14,606,056 |
| CR-2 | 40,000 | 1,468,942 | 26,049,264 | 28,212,260 | 2,162,996 | 28,212,448 |

Geometric hash-table capacity explains the steps in retained memory. Across an
arena the index retains O(distinct object names plus indexed objects) slots, with
an additional O(colliding distinct names) vector payload when collisions occur.
The parser moves existing buckets into the arena without building a second copy.
The earlier report's parse-stage structural-equivalence wording is superseded
by this source-based clarification, while its complete-job headline is retained.
