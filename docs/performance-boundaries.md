# Performance and allocation boundaries

The document-validation, regex-budget and diagnostic-allocation tables retain
measurements of engine revision
`a2e5a380018754bf8b96ffebadd4545fec0a07cb`, with documentation and measurement
harness changes only. The host was macOS 26.6.2 arm64, Rust 1.99.0. Wasm used
wasm-bindgen 0.2.129. Timings are examples, not latency guarantees or CI thresholds.
Prepared-contract and Wasm measurements were updated on 2026-10-15 below.
Use the locked dependencies and release profile to reproduce them.

## Prepared value contracts

Retain a prepared `ValueContract` for repeated validation. Reusing a validated
document or `ValueContracts` context also saves work, but still prepares a fresh
contract on each `prepare` call. The SDK does not transparently cache arbitrary
custom evaluator results.

On 2026-10-15, paired release measurements against revision `7013a36` on macOS
arm64 / Rust 1.99.0 produced the following medians, including shared target-carrier
inspection. Preparation includes schema parsing, minimal document proof, resource
contexts, compilation and destruction; evaluation retains the contract and parsed
value. Twelve adjacent randomized pairs used adaptive batches of at least 80 ms
targeting 120 ms. Ratio intervals are 95% paired bootstrap intervals within this
session, not portable budgets.

| Workload | Preparation before → after µs | Evaluation before → after µs | Evaluation ratio (95% interval) |
| --- | ---: | ---: | --- |
| scalar | 393.21 → 120.29 | 0.572 → 0.084 | 0.146 (0.146–0.148) |
| object | 443.20 → 171.73 | 0.812 → 0.563 | 0.694 (0.692–0.695) |
| array | 387.73 → 124.56 | 5,720.786 → 393.659 | 0.069 (0.068–0.069) |
| refs | 464.92 → 194.99 | 14.994 → 10.063 | 0.672 (0.669–0.675) |
| large-object | 384.22 → 121.55 | 7,517.312 → 991.782 | 0.132 (0.131–0.133) |

The direct SDK/jsonschema-rs 0.58.6 geometric mean over the original five
workloads × four modes (prepare, evaluate, parse, full) fell from **12.49× to
4.98×**. The peer supplies a two-outcome engine-cost reference with different
number and resource semantics. This aggregate is not the earlier 13.1× comparison
against the faster of two peers. Exact decimal-array evaluation changed by
+0.7% (ratio interval 0.996–1.012); integer shortcuts retain its general
arithmetic costs.

A fan-in workload has 100 distinct referring resources pointing to one supplied
integer schema with 5,000 unused definitions. Twelve adjacent fresh-child pairs
measure public `ValueContracts::prepare` alone, after context construction:

| Referrers | Public preparation before → after ms | Ratio (95% interval) |
| --- | ---: | --- |
| 1 | 6.245 → 5.965 | 0.958 (0.932–0.971) |
| 10 | 6.162 → 5.892 | 0.955 (0.941–0.961) |
| 100 | 7.811 → 7.564 | 0.972 (0.957–0.979) |

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
after); that RSS aggregate was not remeasured for this carrier-sharing correction.

| Workload | Preparation peak before → after KiB | Evaluation peak before → after KiB |
| --- | ---: | ---: |
| scalar | 2,268.7 → 2,281.8 | 2,168.7 → 2,198.2 |
| object | 2,274.9 → 2,315.0 | 2,182.0 → 2,211.6 |
| array | 2,503.5 → 2,522.0 | 5,326.3 → 5,355.8 |
| refs | 2,298.8 → 2,340.9 | 2,284.1 → 2,313.6 |
| large-object | 4,100.9 → 4,118.8 | 18,186.1 → 18,215.7 |
| unique-array | 2,502.4 → 2,514.4 | 7,511.6 → 7,093.1 |

Exact integer shortcuts only apply where the original tokens prove the result;
all other tokens retain decimal arithmetic. Lazy reference inspection still uses
the library resolver and the complete explicit-resource identity policy. Standard
indexes cache only pinned immutable documents. Uniqueness verifies exact equality
for every hash collision. Work, depth, regex and cancellation allowances and
original-source diagnostics remain in force.

The separate [benchmark harness](../tools/evaluator-bench/README.md) and
[advisory workflow](../.github/workflows/performance.yml) record outcomes before
timing and keep allocation instrumentation separate. The fan-in fixture includes
cold, compile, retained-context preparation, evaluation and full modes. After
calibration, shared runners will require a confirmed >20% paired regression above
the absolute floor and a lower 95% bound >1.20; dedicated runners use >10% / >1.10.
Heap thresholds are >10% and >128 KiB, allocation counts >5% and >32/op. Noise
requests a rerun. See the harness for exact floors, confirmation and promotion
policy.

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
excluded; complete validation uses fresh parsed owners. This measures neither RSS
nor Wasm capacity. Outputs are under `target/document-diagnostics`.

| Region | Observed maximum | Fixture-specific regression threshold |
| --- | ---: | --- |
| Fixed-schema validity | 17,573,888 bytes | 18 MiB |
| Fixed checks above that input's validity peak | 8,472,586 bytes | 10 MiB |
| Full validation above that input's validity peak | 8,797,823 bytes | 12 MiB |
| Prepared coarse value decisions | 113 bytes | 65,536 bytes |

The absolute full-validation peak was 17,949,677 bytes. Value cases exercise
1,000- and 50,000-element arrays, including partial evaluation. These are bounds
for the recorded fixtures, not all inputs. Document findings separately retain at
most 4,096 entries and 8 MiB of pointers. Upstream value diagnostics have library
allocation and stack limits and may overflow on deep inputs. See
[allocation methodology](../tools/diagnostic-memory-probe/README.md) and
[MSRV and stack qualifications](support.md).
