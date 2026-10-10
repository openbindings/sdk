# OpenBindings SDK

Rust document semantics and a first-class TypeScript/Wasm API for the OpenBindings
0.2 specification line. This repository maintains the reference SDK. Its Rust libraries and TypeScript/Wasm
facade are currently unpublished prerelease packages. Package
version `0.2.0-alpha.1` is independent of the specification version.

Core parses and preserves exact JSON, supports typed authoring, assesses all 13
document rules, indexes operation names and aliases, inspects kinds and references,
and creates value-contract contexts with explicit resources and evaluators. Optional
companions provide a default evaluator, an evaluator qualification kit, and HTTP
discovery. Invocation, synthesis, binding adaptation and application migration are
outside the SDK.

## Installation status

The Rust and TypeScript packages in this repository have not been published.
Existing registry releases of `@openbindings/sdk` belong to the legacy TypeScript
implementation and do not install this Rust-backed API. Build from an exact source
revision or install a locally built archive as described in [source replay](docs/REPLAY.md).
See [the migration boundaries](docs/migration.md) before moving an existing caller.
The [support policy](docs/support.md) names qualified hosts and compatibility
limits. Maintainers can [prepare verified local archives and API references](docs/release-candidate.md)
before a separate publication decision.

## Rust

Start with [Rust first use and replacement](docs/rust-first-use.md) for a complete
document-to-verdict caller, explicit evaluator selection, diagnostics and retained
contracts. The companion ships separate runnable
[first-use](crates/openbindings-json-schema-evaluator/examples/first_use.rs) and
[replacement](crates/openbindings-json-schema-evaluator/examples/replacement.rs)
examples. Core authoring by itself needs no evaluator:

```rust
use openbindings::{DocumentBuilder, Operation};

let mut draft = DocumentBuilder::new();
draft.operations.insert("lookup".into(), Operation::default());
let parsed = draft.build()?;
let assessment = parsed.assess()?;
let validated = assessment.validated().ok_or("document is not conformant")?;
assert!(validated.original_bytes().starts_with(b"{"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Parsing preserves evidence; validation establishes conformance separately. For
operation values, select `openbindings-json-schema-evaluator` explicitly and pass
an immutable `ResourceSet`. `ContractPreparation` distinguishes ready, absent
contract, missing or ambiguous operation, and preparation refusal. Only a ready
contract validates values; `ValueOutcome` distinguishes satisfies, fails and
no-verdict. Core never acquires references.
The broader [six-workflow caller](examples/rust-consumer/src/main.rs) additionally
covers HTTP discovery and evaluator qualification.

## TypeScript

The npm package exposes typed objects and discriminated outcomes over the same
Rust semantics. It includes the Wasm binary; callers do not need Rust or Go.

Begin with the complete [TypeScript first-use guide](packages/typescript/README.md),
the runnable [Node entry](packages/typescript/examples/first-use-node.mjs), or the
[browser page](packages/typescript/examples/first-use.html). Node reads the exported
Wasm asset; workerd passes a compiled module inside an allowed request context.
This smaller initialization and inspection example is for a browser:

```ts
import { initialize, parseDocument } from '@openbindings/sdk';
await initialize();
const result = parseDocument('{"openbindings":"0.2.0","operations":{"ping":{}}}');
if (result.status === 'parsed') {
  using document = result.value;
  console.log(document.operations);
}
```

See the [TypeScript guide](packages/typescript/README.md) for browser/Node/workerd
initialization, exact JSON, disposal, Fetch discovery and Worker scheduling. The
[service lifecycle example](packages/typescript/examples/service-lifecycle.mjs)
shows retained in-flight work and a replacement that is installed only when ready.

## Packages and boundaries

| Package | Responsibility |
| --- | --- |
| `openbindings` | Normative document semantics, authoring, inspection and evaluator contracts |
| `openbindings-json-schema-evaluator` | Optional default JSON Schema 2020-12 value evaluator |
| `openbindings-schema-evaluator-test-support` | Optional 1,566-case adapter qualification kit |
| `openbindings-http-discovery` | Optional portable discovery policy/publication; native HTTP feature |
| `openbindings-wasm` | Internal bridge used by the supported TypeScript facade |
| `@openbindings/sdk` | First-class TypeScript API and included Wasm asset |

`openbindings-internal-json` and five renamed dependency packages are implementation
packages. Their upstream APIs are not SDK extension contracts. The core's private
schema machinery checks the normative document schema; transport and operation
value-evaluator policy remain outside core. See [architecture](docs/architecture.md),
[capabilities](CAPABILITIES.md) and [dependency maintenance](DEPENDENCY-MAINTENANCE.md).

## Build and verify

Use Rust 1.99.0 (the declared MSRV), its `wasm32-unknown-unknown` target,
wasm-bindgen-cli 0.2.129, Node 22.19.0/npm 10.9.3 and Python 3.13+. Dependency
locks are tracked. Initial dependency/tool acquisition requires network access.

```sh
cargo test --locked --workspace --features openbindings-http-discovery/native
python3 tools/verify.py
```

`tools/verify.py` runs formatting, Clippy, native tests/builds, independent corpus
controls and all 435 native cases. `python3 tools/verify.py --browser` additionally
builds the npm facade and runs the full core/evaluator corpus in Chromium and
WebKit. Install their host dependencies first as described in [replay](docs/REPLAY.md).
Tests use task-local loopback listeners and no external application services.

The verifier also compiles core independently with no default features. After
building the facade, `npm run test:package` in `packages/typescript` packs and
installs it into a temporary consumer, then imports its public API and packaged
Wasm to parse and validate a document. CI compares the frozen spec inputs with
the exact applied revision below using `node tools/verify-spec.mjs . <spec-checkout>
<applied-sha>`; the checkout must be at that SHA. Full multi-crate Cargo archive
qualification remains a separate prerelease/package-change check.

CI runs the component checks on Linux, macOS, Windows, Chromium and WebKit.
Inspect [CI results](https://github.com/openbindings/sdk/actions/workflows/ci.yml)
for the intended commit. Node ESM package checks are included; local workerd and
full Cargo archive qualification are separate host/package checks. Record exact
source and artifact identities with additional qualification results.
Do not infer publication or application cutover from this repository. Follow
[RELEASING.md](RELEASING.md) for the separate release process.

## Provenance and licenses

Applied specification revision: `2f7d754dc2da374058cd517064c17e50f7d95d99`.
Comparison Go revision: `c5c6076fcf6bcf30a428c70c00bb4f3cbf6447df`.
The SDK is Apache-2.0; vendored dependencies retain their upstream licenses.
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt) and
[DEPENDENCIES.json](DEPENDENCIES.json) describe native/Wasm dependency closures.
The npm archive carries its own notices, Rust standard-library attribution and
build metadata. Fixture sources carry separate provenance and licenses.

Historical measurements and their exact setup/cache/value-parsing boundaries are explained in [performance boundaries](docs/performance-boundaries.md).

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](docs/api-reference.md).
