# SDK support and compatibility

The Rust-backed SDK is an unpublished `0.2.0-alpha.1` working draft. Use an exact
source revision or verified local archives. The legacy npm `latest` package is a
different implementation; see [migration boundaries](migration.md) before
replacing an existing dependency.

| Environment | Supported path and qualification |
| --- | --- |
| Rust | Toolchain 1.99, edition 2024; Linux, macOS, and Windows native CI. Earlier toolchains are not qualified. Core has no mandatory async runtime. |
| Node | ESM on Node 22 or later; CI pins 22.19.0. Read the exported Wasm asset and initialize explicitly. |
| Browser | Desktop Chromium and WebKit with explicit initialization; use a Worker for potentially expensive work. Firefox, mobile devices, and particular framework/bundler versions require their own qualification. |
| Worker service | Local workerd with a compiled Wasm module and initialization in an allowed request context. This is not evidence of deployed Cloudflare latency, quotas, or service availability. |

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

Before 1.0, minor releases may change public APIs; compatible fixes belong in
patch releases. This does not promise a stable 1.x contract for prereleases.
Read the changelog when updating. Coupled internal packages and Wasm tooling use
exact requirements; ordinary shared dependencies use documented compatible
ranges. See [dependency maintenance](../DEPENDENCY-MAINTENANCE.md).

Report ordinary bugs with a source/package version and small synthetic example
in [GitHub issues](https://github.com/openbindings/sdk/issues). For sensitive
issues, follow [SECURITY.md](../SECURITY.md). The project has no advertised
response-time SLA or supported stable backport branch for this prerelease.
