# OpenBindings SDK

Read, validate and author OpenBindings interface documents in Rust or TypeScript.
Inspect operations and their bindings, then validate caller-facing values against
an operation's input or output value contract. The TypeScript package uses the
same Rust engine through WebAssembly.

The packages are an unpublished `0.2.0-alpha.1` working draft. Use an exact source
checkout or a locally built archive through [source replay](docs/REPLAY.md).
Registry releases of `@openbindings/sdk` belong to the legacy implementation;
see [migration](docs/migration.md) before replacing a dependency.

## Read an operation in Rust

```rust
use openbindings::{Conformance, ParsedDocument};

let document = ParsedDocument::parse(
    br#"{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"]}}}"#,
)?;
let validation = document.validate()?;
assert_eq!(validation.report().conclusion, Conformance::Conformant);
let validated = validation.validated().ok_or("document is not conformant")?;
let operation = validated.resolve_operation("find").ok_or("operation is missing")?;
assert_eq!(operation.key(), "lookup");
# Ok::<(), Box<dyn std::error::Error>>(())
```

Continue with [Rust first use](docs/rust-first-use.md) to prepare a value contract,
validate input, edit opaque values and retain prepared work. Rust needs no runtime
initialization. The evaluator companion's `value_contracts(&validated)` function
supplies default budgets, coarse diagnostics and no extra resources. Prepare with
`context.prepare(name, Direction::Input).into_result()?`, then check JSON text or
bytes with `contract.validate_json(input)?`. The explicit context method remains
available for shared evaluators and supplied resources. The [TypeScript guide](packages/typescript/README.md) starts with a
complete document-to-verdict example and covers initialization and disposal.

## Choose the work you need

Core owns document semantics, exact JSON, typed authoring and operation lookup.
Value evaluation is an explicit companion or a caller-provided implementation.
HTTP discovery acquires interface documents and publishes their immutable bytes.

Rust companions define provider traits and checked callers for source inspection,
interface synthesis and invocation. A source inspector enumerates bindable targets;
a synthesizer produces an interface with coverage; a binding invoker interacts
with a realization; an operation invoker applies document selection and value
contracts. Kind implementations live in separate projects and supply the meaning
of source and binding content. Applications choose providers and acquisition policy.

Start with the crate guide for
[core](crates/openbindings/README.md),
[value evaluation](crates/openbindings-json-schema-evaluator/README.md),
[evaluator qualification](crates/openbindings-schema-evaluator-test-support/README.md),
[HTTP discovery](crates/openbindings-http-discovery/README.md),
[source inspection](crates/openbindings-source-inspection/README.md),
[interface synthesis](crates/openbindings-interface-synthesis/README.md) or
[invocation](crates/openbindings-invocation/README.md).

## Limits and compatibility

Parsing, document conformance and value validation are separate steps. A ready
value contract can still return no-verdict for a value. Core never fetches schema
references. Document validation works without an operation value evaluator;
[the evaluator guide](docs/schema-evaluators.md) explains that boundary.

Rust requires 1.88. The 256 KiB document stack regression is qualified on the
development toolchain; older supported compilers may need larger debug stacks.
TypeScript owners require explicit disposal for timely release. Long synchronous
Wasm work belongs in a Worker when the caller needs responsive cancellation.
Rust role APIs are provisional; public TypeScript role APIs are not provided.

Read [capabilities and limits](CAPABILITIES.md), [host support](docs/support.md),
[performance measurements](docs/performance-boundaries.md) and the
[API reference](docs/api-reference.md). This SDK applies OpenBindings 0.2.
Applied specification revision: `2f7d754dc2da374058cd517064c17e50f7d95d99`.
Package and specification versions are independent.

Repository build and verification instructions are in
[the maintainer guide](docs/maintainers.md).
