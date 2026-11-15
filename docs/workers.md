# Cloudflare Workers

Import `@openbindings/sdk/openbindings.wasm` as a compiled module and await
`initialize(module)` inside the request context. Initialization checks host
entropy before seeding the engine. See the
[workerd service example](../packages/typescript/examples/workerd-service.mjs).

The platform's **128 MB ceiling is per isolate**, shared by concurrent requests,
JavaScript and Wasm. **Wasm linear memory only grows**: whatever one request
makes it grow to stays for the isolate's life. Disposal lets its allocator reuse
space; it does not shrink `memory.buffer.byteLength`.

The tables below measure that byte length in fresh Node v22.23.0 processes on
Darwin arm64, using the package built at `d02c04e9` on 2026-11-14. They reproduce
the lab's Node/workerd workload; all comparable linear-memory figures are within
5% of the lab. Timings differ by more than 5% on many rows and use this tree's
results. Run `node tools/workers-memory/run.mjs` from the repository root against
the built package to regenerate the tables; see the [runner instructions](../tools/workers-memory/README.md).
The instrument is the SDK Wasm instance's `memory.buffer.byteLength`, with a
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
| `preparationLimits.maxSdkReservationBytes` | 8 MiB for up to 3,000 tested records or 4,000 links | Smallest tested row admitting these shapes; use 16 MiB for 16,000 links or 64 MiB for 49,000. Choose by the table below, then measure its linear high-water plus the body's bytes against the host budget. Reservation bytes cannot be subtracted from that budget as if they were actual memory growth. |
| `preparationLimits.maxSdkWorkUnits` | 100,000,000 | Smallest tested decade completing both large preparations when memory permits: 1,000,000 completed the records, but 10,000,000 still refused the chain. The completing chain took 108 ms here; lower and remeasure for a smaller CPU budget. |
| `limits.patternSearchWorkUnits` | 250,000,000 (default) | Refuses one oversized Original pattern search before entry with `pattern-search-work-limit`. Raise it or use `null` to remove the ceiling; either change selects Original. This is not a deadline. |
| `limits.evaluationSdkWorkUnits` | 100,000,000 | Smallest tested decade deciding both large values: 1,000,000 returned no-verdict for the records, as did 10,000,000 for the chain. The hostile pattern returned no-verdict in 3.43 ms with `evaluation-work-limit`, versus 1.858 s before the default search ceiling. Default limits now refuse that search with `pattern-search-work-limit`. Use this for hostile work, not memory sizing. Units are not a deadline. |
| `limits.evaluationSdkReservationBytes` | 4 MiB | Smallest tested allowance completing the 3,000-record value; 1 MiB returned `semantic-analysis-memory-limit`. Qualify it together with preparation and admission. This is SDK accounting, not all allocation. |
| Validation `maxFindings` | 100 | Smallest nonzero retention tested. On 100,000 invalid operations it saved 2.813 MiB versus 4,096 findings; it is secondary to admission limits. Truncation does not change rule conclusions. |

Apply JSON limits to documents, supplied resources **and values**. Ordinary
`evaluate(value)` uses default JSON admission; to enforce these smaller limits,
use `parseJson(text, { limits: jsonLimits })`, check for `parsed`, evaluate its
exact owner, then dispose it. For example, reuse these option objects:

```ts
const MiB = 1024 * 1024;
const jsonLimits = { maxBytes: 4 * MiB, maxNodes: 250_000, maxDepth: 10_000 };
const preparationLimits = {
  maxSdkReservationBytes: 8 * MiB, // up to 3,000 tested records or 4,000 links
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
// schema.prepare(prepareOptions)
// resources.prepare(uri, prepareOptions)
// new ExternalSchemas(entries, { limits: preparationLimits })
// resources.with(uri, exactValue, { limits: preparationLimits })
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

These preparation tables were rerun on 2026-11-15 with held-storage accounting
on owner-rulings `1159bc32`, using Node v22.23.0 on Darwin arm64. The Wasm
SHA-256 is `d32152c2f64ea22a35d0333041e8680d13fe85c39d03a1b13b1005c7b612ccb1`.
The unchanged runner completed all 276 fresh-process observations.

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
Start at 8 MiB for the tested records or up to 4,000 links, then qualify the
application's actual capacity against its host budget. This is a starting
policy for those shapes, not a guarantee that an arbitrary schema fits.

Setting `preparationLimits.maxSdkReservationBytes` gave these outcomes and
linear-memory growth across preparation. Records are 3,000 contained/supplied;
the chain has 49,000 links. Each cell starts from a fresh process.

| Allowance MiB | Records, contained / supplied | Chain |
| ---: | --- | --- |
| 8 | ready / ready, 0 / 0 MiB | refused, 0 MiB |
| 16 | ready / ready, 0 / 0 MiB | refused, 0 MiB |
| 32 | ready / ready, 0 / 0 MiB | refused, 2.938 MiB |
| 64 | ready / ready, 0 / 0 MiB | ready, 2.938 MiB |
| 512 (default) | ready / ready, 41.500 / 41.750 MiB | ready, 73.625 MiB |

Refusals have `failure.kind: "limit"` and
`failure.details.resource: "preparation-sdk-reservation-bytes"`. All allowed a
later small preparation and evaluation in the same instance. No tested growth
exceeded its allowance, but this is **not a cap on linear-memory growth**:
reservations omit allocator overhead, prior admission/registration, and some
dependency allocation. They can also refuse well before that much memory is
allocated. The 512 MiB default exceeds the isolate ceiling.

Current policy selects Original for nondefault preparation allowances. Repeating
the sweep with `evaluationSdkWorkUnits: 100_000_000` produced the same admission outcomes;
at the default preparation allowance it selected Original and grew 0 / 0 MiB
for records and 2.938 MiB for the chain. This cheaper lane is an observation of
current policy, not a memory guarantee from the work cap. Use preparation
reservations for preparation storage and evaluation work units for work.

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

A Wasm runtime trap, including allocation failure, throws `SdkError` with code
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
