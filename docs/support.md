# SDK support and compatibility

The Rust-backed SDK is an unpublished `0.2.0-alpha.1` working draft. Use an exact
source revision or verified local archives. The legacy npm `latest` package is a
different implementation; see [migration boundaries](migration.md) before
replacing an existing dependency.

| Environment | Supported path and qualification |
| --- | --- |
| Rust | MSRV 1.88, edition 2024; independent native/Wasm MSRV CI. Development toolchain 1.99; Linux, macOS, and Windows native CI. Core has no mandatory async runtime. |
| Node | ESM on Node 22.23.0 or later in the 22 line, 24.17.0 or later in the 24 line, or 26.2.0 or later (`engines`: `^22.23.0 || ^24.17.0 || >=26.2.0`), the first releases whose bundled undici rejects a chunked body cut short as its connection closes ([nodejs/undici#5273](https://github.com/nodejs/undici/pull/5273)) instead of handing `discover` the prefix as a complete body. CI pins 22.23.0 and runs the TypeScript tests at the 24 and 26 floors too. Await `initialize()`; Node loads the packaged asset automatically. |
| Browser | Desktop Chromium and WebKit with explicit initialization; use a Worker for potentially expensive work. Firefox, mobile devices, and particular framework/bundler versions require their own qualification. |
| Worker service | Local workerd with a compiled Wasm module and initialization in an allowed request context: `initialize()` checks host entropy before seeding the engine's shared tables. The package's service example also validates and prepares inside the handler. Local workerd enforces neither isolate memory nor CPU limits; this is not evidence of deployed Cloudflare latency, quotas, or service availability. Set admission, preparation and evaluation limits using the [Workers guide](workers.md). |

The current evaluator has Original and compiled execution paths. Non-default
limits and all Explain calls use Original; eligible default Outcome checks may
use the compiled backend. Their stack and cancellation policies differ. See the
[execution policy](../crates/openbindings-json-schema-evaluator/README.md#execution-paths)
and the dated [stack measurements](performance-boundaries.md#stack). No public
stack-byte option or fixed host-stack guarantee is provided.

The Rust and TypeScript APIs use the same engine and outcome meanings. They
follow their languages' ownership conventions: Rust uses ordinary ownership and
borrowing, while TypeScript owners support deterministic `dispose()` and
`Symbol.dispose`. Neither an owner count nor disposal proves that process memory
or Wasm capacity immediately shrinks. See the [API contracts](api-reference.md)
and [TypeScript guide](../packages/typescript/README.md).

Document conformance, parsing, and operation value validation are distinct jobs.
The optional built-in evaluator has a declared JSON Schema capability profile;
unsupported capabilities return no-verdict. Rust applications can supply their
own evaluator and qualify it with the evaluator test-support crate. TypeScript
custom evaluator callbacks are not currently provided. The complete boundaries
are in [CAPABILITIES.md](../CAPABILITIES.md).

For ordinary value validation, use `evaluate`, the outcome-only check. `explain`
is an explicit per-call choice for diagnostic evidence. Diagnostics have their
own bounds and may be incomplete even when the failure is established.
An empty problem list never turns an established failure into success.
See the [evaluator policy](../crates/openbindings-json-schema-evaluator/README.md).

The default evaluator uses bounded reasoning over original schema locations,
exact values and evaluation annotations. It can
establish `Undefined` when validity depends on a known undefined result under
the specification. A missing resource, unsupported feature or exhausted allowance
does not by itself establish undefinedness. Such cases remain `NoVerdict` unless
the evaluator can establish a result independently of them. The
[semantic dependence policy](../crates/openbindings-json-schema-evaluator/README.md#results-and-semantic-dependence)
describes the supported recursive certificates and their limits. Preparation
failure judges no value, and remains separate from these per-value outcomes.

Preparing a schema from an untrusted party can take more memory than a host
may want, as with any JSON Schema library: the evaluator bounds what the SDK
itself reads and writes, not the library's compiled program (see the
[evaluator limits](../crates/openbindings-json-schema-evaluator/README.md#limits-and-cancellation)).
On native hosts, prepare such schemas in a separate process with an
operating-system memory limit. Cloudflare Workers need explicit SDK allowances
and deployment qualification; local workerd has no such process limit. In
TypeScript an allocation failure throws an `SdkError` with code `wasm-trap`,
and every later call on that instance fails with the same code. Treat it as
fatal for the isolate; see [Workers recovery](workers.md#memory-exhaustion).
Value evaluation uses static compiled admission or Original heap frames and
sampled stack checks. A caller-selected cumulative work cap selects Original.
Original pattern search has a default 250,000,000-unit ceiling per uncancellable
search; oversized anchored searches return `pattern-search-work-limit` before
running. Unanchored searches can establish a match within a bounded prefix; an
inconclusive prefix returns the same limit code.
Changing `pattern_search_work_units` / `patternSearchWorkUnits` from its default
selects Original; `None` / `null` removes this ceiling. SDK limits
are not a hard CPU deadline or a whole-process memory ceiling. Comparison,
preparation and pull relationship work budgets are optional and off by default.
A time bound is the caller's choice through cancellation or the corresponding
`Option<usize>` field. Memory allowances retain defaults (64 MiB comparison,
512 MiB preparation, 64 MiB pull relationships) for conservative reservations of
storage held at once. Scratch releases its reservations when dropped; retained
programs, indexes and reports stay charged. See the dated
[host-specific 250 ms preparation suggestion](performance-boundaries.md#limits-principle-november-2026).
The 512 MiB preparation default exceeds a Worker's 128 MB isolate ceiling.
TypeScript sets a smaller reservation through `preparationLimits` on either
prepare method, and through `{ limits }` on `ExternalSchemas` registration.
These reservations do not cap Wasm linear memory or dependency allocations.
