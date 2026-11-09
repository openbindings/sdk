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
| Worker service | Local workerd with a compiled Wasm module and initialization in an allowed request context. This is not evidence of deployed Cloudflare latency, quotas, or service availability. |

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
Prepare such schemas in an isolated worker or process with an operating-system
memory limit. Value evaluation uses static compiled admission or Original heap frames and
sampled stack checks. A caller-selected work cap selects Original. SDK limits
are not a hard CPU deadline or a whole-process memory ceiling.
