# Performance and allocation boundaries

These measurements characterize engine revision
`a2e5a380018754bf8b96ffebadd4545fec0a07cb`, with documentation and measurement
harness changes only. The host was macOS 26.6.2 arm64, Rust 1.99.0. Wasm used
wasm-bindgen 0.2.129. Timings are examples, not latency guarantees or CI thresholds.
Use the locked dependencies and release profile to reproduce them.

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

The measured asset is **6,043,108 bytes**, or **1,654,055 bytes** with gzip level 9.
Its SHA-256 is `d059fc1e48ac878b0034316120440707343eea89f05a44f83b52a23b9b4c7c4f`.
The build optimizes the private bridge for size and removes the Wasm name section.
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
