# Cloudflare Workers

Import `@openbindings/sdk/openbindings.wasm` as a compiled module and await
`initialize(module)` inside the request context. Initialization checks host
entropy before seeding the engine. See the
[workerd service example](../packages/typescript/examples/workerd-service.mjs).

The platform's **128 MB ceiling is per isolate**, shared by concurrent requests,
JavaScript and Wasm. **Wasm linear memory only grows**: whatever one request
makes it grow to stays for the isolate's life. Disposal lets its allocator reuse
space; it does not shrink `memory.buffer.byteLength`.

The tables below measure linear-memory capacity in fresh Node v22.23.0
processes on Darwin arm64 on 2026-11-15, using the registration repair based on
`3fcabf67`. The release Wasm SHA-256 is
`c2cf03227365463e11d87ee37eb08186a800849e7e818cd87cd757f6ad15b1f8`.
Run `node tools/workers-memory/run.mjs` from the repository root against the
built package to regenerate the tables; see the [runner instructions](../tools/workers-memory/README.md).
The runner completed 363 fresh-process observations, including the full recipe.
A rerun on the combined 2026-11-15 repairs (Wasm SHA-256
`ecfbb26e4056caa8b65a55a359665cb4d85b1f042aed544ad5f07d0de9bfeee7`, after the pattern and evaluator repairs) reproduced every
linear-memory figure within 0.063 MiB; the default 49,000-link chain varies by
0.5 MiB between runs (73.625 or 74.125 MiB of growth) on either build.
Timings are single cold observations, separate from the repeated registration
performance comparison. The instrument is the SDK Wasm instance's `memory.buffer.byteLength`, with a
fresh process per row and per preparation-allowance search trial.

**Local workerd enforces neither an isolate memory limit nor a CPU limit.**
Measured in local workerd on 2026-11-14 at `2351ac2`, process RSS was 33.3 MiB
for an empty Worker, 81.2 MiB with the SDK loaded and 121.5 MiB after first use.
First validation took 59-104 ms and first preparation 18-29 ms. These are local
process/timing observations, not platform quotas or deployed latency. Whether
the platform counts compiled Wasm code toward the isolate ceiling is unmeasured.

## Limits to set

Start with a **64 MiB target for linear memory plus one UTF-8 request body**,
leaving roughly half the isolate for other use. This is a measured sizing
heuristic, not a guarantee for arbitrary schemas, dependency allocations,
concurrent bodies or compiled code. Enforce the body limit while reading the
stream, before buffering it all; an SDK byte limit cannot bound the host's copy.

| Setting | Starting value | Rule and measurement |
| --- | ---: | --- |
| JSON `maxBytes` | 4 MiB | Choose the largest tested input whose worst shape leaves preparation/evaluation room. Copied identifiers at 4 MiB reached 45.875 MiB; with a body, about 50 MiB. At 8 MiB they reached 59.875 MiB, exceeding the 64 MiB target once the body is added. A byte refusal still grew linear memory by 4.063 MiB at the 4 MiB limit. |
| JSON `maxNodes` | 250,000 | Reserve about 16 MiB for node-dense admission: the roughly 250,000-node shapes reached 14.688-15.438 MiB, and refusal at this cap reached 16.125 MiB. Scale from measured node-dense shapes, not document bytes alone. |
| JSON `maxDepth` | 10,000 (default) | Keep the existing qualified depth ceiling: 9,990 array levels grew memory by 1.125 MiB. This is not a stack-byte guarantee. |
| `preparationLimits.maxSdkReservationBytes` | 16 MiB for up to 3,000 supplied records or 16,000 links | Registration includes the supplied source arena: 3,000 records need 9.632 MiB, so 8 MiB refuses. Contained records need 6.312 MiB to prepare. Use 64 MiB for 49,000 links and also raise evaluation storage to 8 MiB. Measure total capacity plus body bytes against the host budget. Reservation bytes are not memory growth. |
| `preparationLimits.maxSdkWorkUnits` | 100,000,000 | Smallest tested decade completing both large preparations when memory permits: 1,000,000 completed the records, but 10,000,000 still refused the chain. The complete qualified chain recipe took 287.359-303.201 ms, including 116.793-130.887 ms preparing. Lower and remeasure for a smaller CPU budget. |
| `limits.patternSearchWorkUnits` | 250,000,000 (default) | Bounds one Original pattern search; insufficient work returns `pattern-search-work-limit`. Raise it or use `null` to remove the ceiling; either change selects Original. This is not a deadline. |
| `limits.evaluationSdkWorkUnits` | 100,000,000 | Smallest tested decade deciding both large values: 1,000,000 returned no-verdict for the records, as did 10,000,000 for the chain. The hostile pattern returned no-verdict in 3.526 ms with `evaluation-work-limit`, versus 1.858 s before the default search ceiling. Default limits now refuse that search with `pattern-search-work-limit`. Use this for hostile work, not memory sizing. Units are not a deadline. |
| `limits.evaluationSdkReservationBytes` | 4 MiB | Completes the 3,000-record value and 16,000-link chain; 1 MiB refuses the records. The 49,000-link chain needs 8 MiB in the complete recipe; at 4 MiB it returns `semantic-analysis-memory-limit`. This is SDK accounting, not all allocation. |
| Validation `maxFindings` | 100 | Smallest nonzero retention tested. On 100,000 invalid operations it saved 2.813 MiB versus 4,096 findings; it is secondary to admission limits. Truncation does not change rule conclusions. |

These nondefault evaluation limits select the SDK's own Original lane instead
of the compiled dependency lane. The work and reservation accounting belongs to
that SDK lane; it is not a deadline or a cap on all isolate allocation.

At a request boundary, distinguish `no-verdict` from `fails`. A Worker must not
accept an undecided value or label it schema-invalid. For example, return 503
with the fixed `detail.code` for `no-verdict`, 422 for `fails`, and 200 for
`satisfies`; choose a separate error response for `undefined`. Apply the same
policy to `evaluate` and `explain`, and bound retries rather than automatically
raising limits. The service example implements this mapping.

Apply JSON limits to documents, supplied resources **and values**. Ordinary
`evaluate(value)` uses default JSON admission; to enforce these smaller limits,
use `parseJson(text, { limits: jsonLimits })`, check for `parsed`, evaluate its
exact owner, then dispose it. For example, reuse these option objects:

```ts
const MiB = 1024 * 1024;
const jsonLimits = { maxBytes: 4 * MiB, maxNodes: 250_000, maxDepth: 10_000 };
const preparationLimits = {
  maxSdkReservationBytes: 16 * MiB, // up to 3,000 supplied records or 16,000 links
  maxSdkWorkUnits: 100_000_000,
};
const prepareOptions = {
  preparationLimits,
  limits: {
    evaluationSdkWorkUnits: 100_000_000,
    evaluationSdkReservationBytes: 4 * MiB,
  },
};
// Document.parse(text, { limits: jsonLimits })
// document.validate({ limits: { maxFindings: 100 } })
// parseJson(resourceText, { limits: jsonLimits })
// new ExternalSchemas(entries, { limits: preparationLimits })
// resources.with(uri, exactValue, { limits: preparationLimits })
// schema.prepare({ ...prepareOptions, resources })
// resources.prepare(uri, prepareOptions)
// parseJson(valueText, { limits: jsonLimits })
// prepared.evaluate(exactValue)
```

`validateDocument(text, { limits: jsonLimits })` accepts admission limits only;
use `Document.parse` then `Document.validate` to limit findings. Registration
and preparation choose separate allowances; a registration policy does not
carry into a later prepare call. Bound aggregate supplied resources and cached
owners too. Pattern, resource-count and reference-report allowances were not
qualified by this sweep; measure and budget them separately before admitting
workloads that use them. Increasing one allowance requires revisiting the
remaining budget and qualifying the deployed Worker.

## Document memory

Default-limit validation, MiB of linear memory after the call. "Many" means
operations with 600-character descriptions and two-property inputs; "nested"
means up to 2,000 object-schema levels with sibling string properties. Requested
sizes are rounded down by the generators; the runner records exact byte counts.

| Requested size, MiB | Many | Nested |
| ---: | ---: | ---: |
| 1 | 4.750 | 4.500 |
| 2 | 7.063 | 7.063 |
| 8 | 22.188 | 22.063 |
| 16 | 42.250 | 41.938 |
| 24 | 74.313 | 74.313 |
| 32 | 82.313 | 82.125 |

| Other default-limit shape | Input bytes | Linear MiB |
| --- | ---: | ---: |
| 999,000 zeros, about a million nodes | 1,998,051 | 52.250 |
| 499,000 object members, about a million nodes | 5,876,941 | 55.938 |
| 333,320 copied `$id` paths | 33,262,164 | 209.438 |

Ordinary shapes would fit the 64 MiB target at 16 MiB of input
(`42.25 + 16 = 58.25`), but node-dense and copied-identifier shapes invalidate
that as a general admission rule. Set bytes and nodes together.

## Preparation memory

The tables below were rerun with the same release build as the document and
complete-recipe tables. The default-limit capacities and contained preparation
thresholds reproduced the previous measurements; the supplied sweep now also
applies its selected allowance during registration.

Each record has eight properties, two required, with integer, string, number,
boolean, array and enum checks and no additional properties. "Supplied" here
means a contained operation input referring to an `ExternalSchemas` resource,
not direct supplied-root preparation. "Distinct" adds the record index to each
property name. Values satisfy the schemas. All calls below use default limits.

| Records | Placement | Before prepare MiB | Growth MiB | After prepare MiB |
| ---: | --- | ---: | ---: | ---: |
| 100 | contained | 2.938 | 1.188 | 4.125 |
| 1,000 | contained | 5.438 | 14.000 | 19.438 |
| 3,000 | contained | 15.250 | 41.500 | 56.750 |
| 100 | supplied | 2.688 | 1.438 | 4.125 |
| 1,000 | supplied | 5.750 | 13.750 | 19.500 |
| 3,000 | supplied | 15.250 | 41.750 | 57.000 |
| 3,000 | contained, distinct | 15.375 | 42.000 | 57.375 |
| 3,000 | supplied, distinct | 15.375 | 42.250 | 57.625 |

| Pointer-chain links, ending in integer | Before prepare MiB | Growth MiB | After prepare MiB |
| ---: | ---: | ---: | ---: |
| 1,000 | 2.875 | 1.125 | 4.000 |
| 4,000 | 3.375 | 6.188 | 9.563 |
| 16,000 | 5.625 | 26.563 | 32.188 |
| 32,000 | 9.250 | 53.188 | 62.438 |
| 49,000 | 15.938 | 73.625 | 89.563 |

Choose preparation's reservation by the workload it admits. Among contained
records at 10, 100, 300, 1,000 and 3,000 and chains at 1,000, 4,000, 16,000 and
49,000 links, these are the largest **tested** completions, not absolute maxima.
Only `preparationLimits.maxSdkReservationBytes` is changed in this table:

| Allowance MiB | Contained records | Chain links |
| ---: | ---: | ---: |
| 8 | 3,000 | 4,000 |
| 16 | 3,000 | 16,000 |
| 32 | 3,000 | 16,000 |
| 64 | 3,000 | 49,000 |

The rule is to start with the smallest row that admits the application's
required shape, then measure total linear capacity with its real document and
value, leaving room for host memory and other owners. If that exceeds the host
budget, reduce the admitted workload. For shapes beyond these rows, use the
runner's completing-allowance search and qualify actual capacity separately.
Preparation charges held collection capacities and shared owners, with
version-bound profiles for fixed compiler and dependency construction. Native
counting-allocator qualification on 43 record/resource and cold-dialect samples
measures charge/peak requested held heap at 1.000735-1.690947; a separate large
`const` sample measures 1.340629. These factors exclude allocator metadata,
stack, RSS and reallocation overlap, and do not qualify the chain family.

The completing allowance divided by Wasm growth is a different ratio: 4.29-15.06
on the nonzero-growth rows below, rather than the previous 34-91. Zero growth
reuses capacity committed during admission and does not mean zero allocation.
Binary-search brackets are within 1%; these are completing upper endpoints,
not exact byte minima.

| Shape | Size | Refused MiB | Completing MiB | Growth MiB | Allowance / growth |
| --- | ---: | ---: | ---: | ---: | ---: |
| records | 10 | 0.089 | 0.090 | 0.000 | undefined |
| records | 100 | 0.307 | 0.309 | 0.000 | undefined |
| records | 300 | 0.797 | 0.805 | 0.188 | 4.29 |
| records | 1,000 | 2.109 | 2.125 | 0.438 | 4.86 |
| records | 3,000 | 6.281 | 6.312 | 0.000 | undefined |
| chain | 1,000 | 1.078 | 1.086 | 0.000 | undefined |
| chain | 4,000 | 3.781 | 3.812 | 0.313 | 12.20 |
| chain | 16,000 | 14.750 | 14.875 | 2.250 | 6.61 |
| chain | 49,000 | 44.000 | 44.250 | 2.938 | 15.06 |

The 3,000-record allowance falls from the previous 105 MiB to 6.312 MiB.
Start at 8 MiB for contained records or up to 4,000 links. Use 16 MiB when
registering 3,000 supplied records or preparing 16,000 links, then qualify
the application's actual capacity against its host budget. This is a starting
policy for those shapes, not a guarantee that an arbitrary schema fits.

Setting `preparationLimits.maxSdkReservationBytes` gave these outcomes and
linear-memory growth across preparation, or across registration if it refuses.
Registration receives the same allowance. Records are 3,000 contained/supplied;
the chain has 49,000 links. Each cell starts from a fresh process.

| Allowance MiB | Records, contained / supplied | Chain |
| ---: | --- | --- |
| 8 | prepare ready / register refused, 0 / 0 MiB | prepare refused, 0 MiB |
| 16 | ready / ready, 0 / 0 MiB | refused, 0 MiB |
| 32 | ready / ready, 0 / 0 MiB | refused, 2.938 MiB |
| 64 | ready / ready, 0 / 0 MiB | ready, 2.938 MiB |
| 512 (default) | ready / ready, 41.500 / 41.750 MiB | ready, 73.625 MiB |

Preparation refusals have `failure.kind: "limit"` and
`failure.details.resource: "preparation-sdk-reservation-bytes"`. Registration
throws `SdkError` with code `resource`. All allowed a
later small preparation and evaluation in the same instance. No tested growth
exceeded its allowance, but this is **not a cap on linear-memory growth**:
reservations omit allocator overhead, prior admission/registration, and some
dependency allocation. They can also refuse well before that much memory is
allocated. The 512 MiB default exceeds the isolate ceiling.

Current policy selects Original for nondefault preparation allowances. Repeating
the sweep with `evaluationSdkWorkUnits: 100_000_000` produced the same admission outcomes;
including the supplied registration refusal at 8 MiB. At the default preparation
allowance it selected Original and grew 0 / 0 MiB
for records and 2.938 MiB for the chain. This cheaper lane is an observation of
current policy, not a memory guarantee from the work cap. Use preparation
reservations for preparation storage and evaluation work units for work.

## Registration memory

Registration keeps the supplied JSON arena and its identity index. It counts each
shared arena once, follows collection capacity and releases temporary index and
normalization storage. That retained source is already available to preparation,
so registration and preparation have different minimum allowances.

These byte-exact Node searches confirm that the successful allowance minus one
byte refuses; both use 100,000,000 registration work units. Before is `3fcabf67`.

| Supplied records | Before MiB | After MiB |
| ---: | ---: | ---: |
| 1,000 | 6.023 | 2.500 |
| 3,000 | 19.691 | 9.632 |

The native counting allocator measures the source arena once plus peak held
registration storage, excluding released parsing scratch. Across 39 record,
chain, shared-owner, identifier and subtree samples, charge/held-heap is
1.000000-1.102068, within the same 1.0-1.7 qualification used for preparation.
Native 1,000 and 3,000 records need exactly 3,146,176 and 12,198,288 bytes,
both 1.000000 times their measured held heap. Before, the ratios were 2.174401
and 1.864741. Native widths differ from Wasm; these are not Wasm-growth ratios.
The repeated release registration comparison improved production native time 17-22% and
Node time 19-21%; native held memory fell by 104 bytes per tested shape and
Wasm capacity was unchanged. Commands and instruments are in the
[runner instructions](../tools/workers-memory/README.md#registration-and-the-complete-guide-recipe).

## Complete guide recipe

Every documented shape below uses the code block's JSON limits for the document,
supplied resource and value, 100 retained validation findings, the selected
registration limits, and the preparation/evaluation limits together. Each row
runs twice in fresh processes: constructor / `with()`. All owners remain alive
until the synchronous recipe finishes. Both modes have the same outcome. Values separated by `/` in either measurement
column are constructor / `with()`.

The starting options are 16 MiB for registration/preparation and 4 MiB for
evaluation. Explicit larger-chain rows change only the allowances shown.
A refusal stops the recipe at that stage. Contained workloads register one
Boolean resource to exercise both registration APIs. Annotation-only array,
object and deep JSON documents also select that Boolean schema for preparation
and evaluation; other contained workloads select their documented schema.
The original document remains alive. Nested-schema admission can succeed while
preparation refuses its independent schema-depth limit. The 250,000 array and
125,000 object shapes exceed 250,000 total nodes once the document wrapper is
included. Copied-ID and findings rows also show why admission-only results do
not promise a completed request.

| Workload | Outcome / stopping stage | Linear high-water MiB | Complete recipe ms |
| --- | --- | ---: | ---: |
| many, 1 MiB | satisfies | 4.750 | 78.839 / 70.456 |
| nested, 1 MiB | prepare: schema-depth-limit | 4.750 | 72.695 / 70.615 |
| many, 2 MiB | satisfies | 7.500 | 102.927 / 99.158 |
| nested, 2 MiB | prepare: schema-depth-limit | 7.500 | 85.313 / 81.714 |
| many, 8 MiB | parse document: byte-limit | 5.938 | 31.504 / 29.657 |
| nested, 8 MiB | parse document: byte-limit | 5.938 | 26.908 / 27.605 |
| many, 16 MiB | parse document: byte-limit | 5.938 | 30.118 / 31.241 |
| nested, 16 MiB | parse document: byte-limit | 5.938 | 27.942 / 29.026 |
| many, 24 MiB | parse document: byte-limit | 5.938 | 33.001 / 32.391 |
| nested, 24 MiB | parse document: byte-limit | 5.938 | 29.403 / 29.070 |
| many, 32 MiB | parse document: byte-limit | 5.938 | 34.075 / 36.101 |
| nested, 32 MiB | parse document: byte-limit | 5.938 | 30.171 / 30.593 |
| array, 999,000 | parse document: node-limit | 16.125 | 19.911 / 26.118 |
| object, 499,000 | parse document: byte-limit | 5.938 | 29.895 / 27.208 |
| 333,320 copied IDs, about 32 MiB | parse document: byte-limit | 5.938 | 37.432 / 32.158 |
| 40,000 copied IDs, about 4 MiB | prepare: preparation-sdk-reservation-bytes | 30.312 | 223.590 / 215.276 |
| 80,000 copied IDs, about 4 MiB, with fragments | validate: schema-mismatch | 45.875 | 306.425 / 336.509 |
| 80,000 copied IDs, about 8 MiB, with fragments | parse document: byte-limit | 5.938 | 29.550 / 28.404 |
| array, 250,000 | parse document: node-limit | 14.688 | 12.608 / 12.548 |
| object, 125,000 | parse document: node-limit | 15.438 | 22.324 / 17.829 |
| deep, 9,990 | satisfies | 3.250 | 50.361 / 55.791 |
| findings, 100,000 | parse document: node-limit | 16.188 | 21.514 / 21.670 |
| 10 records, contained | satisfies | 2.750 | 84.413 / 52.867 |
| 100 records, contained | satisfies | 2.938 | 59.789 / 71.099 |
| 300 records, contained | satisfies | 3.938 / 4.062 | 70.635 / 71.381 |
| 1,000 records, contained | satisfies | 7.562 | 110.050 / 103.105 |
| 3,000 records, contained | satisfies | 17.938 | 176.638 / 204.031 |
| 3,000 records, contained, distinct | satisfies | 18.062 | 194.913 / 210.845 |
| 10 records, supplied | satisfies | 2.688 | 53.261 / 58.291 |
| 100 records, supplied | satisfies | 2.688 | 61.669 / 64.214 |
| 300 records, supplied | satisfies | 4.000 / 3.875 | 73.962 / 79.957 |
| 1,000 records, supplied | satisfies | 7.000 | 110.132 / 112.704 |
| 3,000 records, supplied | satisfies | 17.625 | 220.149 / 210.401 |
| 3,000 records, supplied, distinct | satisfies | 17.750 | 245.184 / 228.045 |
| 1,000 links | satisfies | 2.875 | 59.397 / 62.397 |
| 4,000 links | satisfies | 3.750 | 77.846 / 85.646 |
| 16,000 links | satisfies | 8.312 | 137.411 / 149.242 |
| 32,000 links | prepare: preparation-sdk-reservation-bytes | 12.375 | 157.573 / 149.419 |
| 49,000 links | prepare: preparation-sdk-reservation-bytes | 18.188 | 190.571 / 189.511 |
| hostile pattern | evaluate: evaluation-work-limit | 10.250 | 59.259 / 56.930 |
| 49,000 links, 64 MiB preparation, 4 MiB evaluation | evaluate: semantic-analysis-memory-limit | 21.125 | 284.772 / 287.593 |
| 32,000 links, 64 MiB preparation, 8 MiB evaluation | satisfies | 14.375 | 216.934 / 218.320 |
| 49,000 links, 64 MiB preparation, 8 MiB evaluation | satisfies | 22.000 / 22.938 | 287.359 / 303.201 |

The 3,000-record supplied recipe reaches 17.625 MiB linear capacity, or
17.750 MiB with distinct names, with both registration APIs. The qualified
49,000-link recipe reaches 22.000-22.938 MiB and completes in 287.359-303.201 ms.
Its document is 1,790,902 UTF-8 bytes; retain room for the host body, supplied
resource/value texts, JavaScript objects and concurrent requests as well.
These complete-recipe observations support the starting settings only for the
listed completing shapes; they are not a blanket schema admission guarantee.

## Handler habits

- Dispose every owner with `using` or `try`/`finally`: parsed values, documents,
  conformance proofs, schemas, prepared schemas and resource sets.
  Canonical entry reads such as `operations()` and `operation(key)` return detached,
  runtime-frozen records with no handles and nothing to dispose. Keep records across
  an `await` freely; navigation and exact payload queries use a live proof and primary keys.
  The [synchronous disposal probe](../tools/workers-memory/README.md#entry-disposal)
  measures 10,000 undisposed `operations()` calls in one turn.
- A forgotten `Schema` or `ExactJson` acquired from a document keeps its whole document
  in Wasm memory until disposed or finalized, even after the proof and document handles
  are disposed. Finalizers are best effort. Entry records do not pin that storage.
- Do not hold owners across an `await`. Read the bounded body first,
  finish SDK work synchronously, copy needed plain results, dispose, then await
  further I/O. Concurrent retained owners add to the same isolate's high-water mark.
- A timer-driven `AbortSignal` cannot interrupt synchronous SDK work: the timer
  cannot run until the call returns. The default per-search ceiling now refuses
  the measured hostile pattern before its uncancellable search runs. It does
  not bound an entire request or let timers interrupt admitted searches. Keep
  a cumulative work cap for aggregate work. A pre-aborted signal works.
  Before the ceiling, local workerd on 2026-11-14 measured a hostile evaluation stalling a ping
  to the same isolate for 1,671-1,947 ms; this is not deployed-platform timing.

## Memory exhaustion

A trap from SDK Wasm execution, including allocation failure, throws `SdkError` with code
`wasm-trap` and the fixed message `The OpenBindings WebAssembly instance trapped
and can no longer be used.` The SDK marks this module instance unusable: every
later SDK operation, including validation and preparation, fails immediately with
that code. `initialize()` rejects with the same code and cannot replace it.
Disposing existing owners remains safe and idempotent; cleanup skips Wasm after
a trap, and finalizers never throw. The original runtime error is not attached.

The failure witness is a 49,000-link preparation under a 64 MiB Wasm cap.
Before this boundary guard, its bare `RuntimeError: unreachable` left preparation
broken while small-document validation still worked. That behavior was reproduced
in Node and local workerd on 2026-11-14. The package's child-Node regression now
caps memory with `--wasm-max-mem-pages=1024` and checks the stable error, immediate
follow-up refusals and safe disposal.

Treat `wasm-trap` as fatal for the isolate: return an error such as 503 and resume
only in a fresh isolate. Whether the deployed platform recycles an isolate after
a trap is unmeasured.

For local failure probes, workerd's
`v8Flags = ["--wasm-max-mem-pages=2048"]` caps linear memory at 128 MiB
(65,536 bytes per page). It excludes the JS heap and compiled code and cannot
simulate the platform's complete memory accounting or CPU enforcement.
