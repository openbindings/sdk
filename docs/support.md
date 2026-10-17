# SDK support and compatibility

The Rust-backed SDK is an unpublished `0.2.0-alpha.1` working draft. Use an exact
source revision or verified local archives. The legacy npm `latest` package is a
different implementation; see [migration boundaries](migration.md) before
replacing an existing dependency.

| Environment | Supported path and qualification |
| --- | --- |
| Rust | MSRV 1.88, edition 2024; independent native/Wasm MSRV CI. Development toolchain 1.99; Linux, macOS, and Windows native CI. Core has no mandatory async runtime. |
| Node | ESM on Node 22 or later; CI pins 22.19.0. Read the exported Wasm asset and initialize explicitly. |
| Browser | Desktop Chromium and WebKit with explicit initialization; use a Worker for potentially expensive work. Firefox, mobile devices, and particular framework/bundler versions require their own qualification. |
| Worker service | Local workerd with a compiled Wasm module and initialization in an allowed request context. This is not evidence of deployed Cloudflare latency, quotas, or service availability. |

The 256 KiB small-stack document regression, and preparation of value contracts
whose patterns nest to the default `pattern_depth`, are qualified on the
development toolchain. Older supported compilers may need larger stacks in debug builds.
MSRV CI runs all workspace tests except the exact stack-margin tests selected by
[this inventory](../tools/stack-margin-tests.json), with reasons and scope for the
vendored tests too. Development-toolchain jobs retain the workspace stack tests.

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

For untrusted request validation, use `validate`, the cheap check. `explain` is an
explicit per-call choice for callers that accept library allocation and stack
limits; it can allocate in proportion to inputs and overflow on deep values.
An empty problem list never turns an established failure into success.
See the [evaluator policy](../crates/openbindings-json-schema-evaluator/README.md).

The default evaluator can decide positive contracts containing known static
reference defects in present resources. Agreement across completions produces
Satisfies or Fails; disagreeing concrete completions with proven defect provenance
produce Undefined. Missing resources, unsupported features and exhausted budgets
keep their distinct refusal reasons. This support excludes malformed `$id`,
dynamic defects and annotation-dependent cases. An unused malformed `$ref` sibling
is isolated; an unused malformed `$id` sibling still refuses. See the
[partial-resource and reference-defect boundaries](partial-resource-bounds.md).
