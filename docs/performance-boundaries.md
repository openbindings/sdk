# Historical SDK performance boundaries

This is an additive clarification of the immutable 2026-10-08 migration evidence
at `ed2bf08718e7995a06d26b1a3534c326eb8bacc7`, not a new Go/Rust measurement.
The end-to-end headline (88.13 ms Go / 19.83 ms Rust; 77.5% elapsed reduction,
about 9% higher fresh-process peak RSS) remains the comparison of equivalent
complete work on its frozen 1,000-schema fixture. Independent numeric recomputation is a separate check; this note states
boundaries from the preserved harness source.

| Series | Timed Rust work | Timed Go work | Valid interpretation |
| --- | --- | --- | --- |
| e2e | Parse, assess, new context, prepare, parse value, validate | Validate document, resolve context, compile, parse value, validate | Complete comparable caller work; headline. |
| compile | New context plus prepare on an existing document | Compile on an existing resolved context | Different setup inclusion; not an isolated compiler speed ratio. |
| reuse | Prepare again in a warmed context with caching | Repeated compilation in an existing resolved context | Public cache-policy comparison; not equal cold compilation. |
| multi | Parse document, new context, 16 preparations/validations; one `{}` value parsed outside timer | Parse document, resolve, 16 compilations/validations; `{}` parsed within each iteration | Shared caller strategy with different value parsing; not an exact equivalent isolated stage ratio. |
| parse | Raw ParsedDocument parsing | ParseDocument includes supported-version/structural-model checks | Descriptive API boundary only; not an equal structural-check comparison. |
| retained | Validate an already parsed value against a retained contract | Same retained-value strategy | Reused validation; excludes preparation and value parse. |
| empty/full | Parse value then validate against retained contract | ValidateJSON parses value then validates retained contract | Value conversion is inside both timers. |

Rust's context includes resource indexing/projection setup. Native `time -l` RSS
includes process setup and warmup; it is not allocated-byte traffic. Browser
initialization was in fresh contexts, not necessarily fresh browser processes;
local uncompressed transport is not an internet loading estimate. JS/Wasm and
native Rust timings do not establish a speedup over a pure TypeScript engine.
The earlier report's parse-stage structural-equivalence wording is superseded
by this source-based clarification, while its complete-job headline is retained.
