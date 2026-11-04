# OpenBindings SDK

Read, validate and author OpenBindings interface documents in Rust or TypeScript.
Inspect operations and their bindings, then validate caller-facing values against
an operation's input or output schema. The TypeScript package uses the
same Rust engine through WebAssembly.

The packages are an unpublished `0.2.0-alpha.1` working draft. Use an exact source
checkout or a locally built archive through [source replay](docs/REPLAY.md).
Registry releases of `@openbindings/sdk` belong to the legacy implementation;
see [current integration boundaries](docs/migration.md) when selecting a dependency.

## Read an operation in Rust

```rust
use openbindings::{Conformance, validate_document};

let validation = validate_document(
    br#"{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"]}}}"#,
)?;
assert_eq!(validation.report().conclusion, Conformance::Conformant);
let conformant = validation.conformant().ok_or("document is not conformant")?;
let operation = conformant.resolve_operation("find").ok_or("operation is missing")?;
assert_eq!(operation.key(), "lookup");
for operation in conformant.operations() {
    // Each view is one operation, in key order; aliases are None when absent.
    let aliases: Vec<&str> = operation.aliases().into_iter().flatten().collect();
    assert_eq!((operation.key(), aliases), ("lookup", vec!["find"]));
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`validate_document` judges the bytes as they are: text that is not JSON is an
OBI-01 violation in the report, located like any other finding, and only an
unsupported version is an `Err`. Continue with [Rust first use](docs/rust-first-use.md) to prepare a schema,
validate input, edit opaque values and retain prepared work. Rust needs no runtime
initialization. Canonical operations expose contextual `Schema` owners.
`schema.prepare(&evaluator, &external_schemas, &preparation_limits)` returns a
prepared validator or a preparation failure, separately from value results.
`prepared.evaluate_json(input)?` admits and checks exact JSON; `explain(&value)`
requests diagnostics at original instance and schema locations. The
[TypeScript guide](packages/typescript/README.md) starts with a
complete document-to-verdict example and covers initialization and disposal.

To edit, parse a `Document` with `Document::parse` (or create one with
`Document::new`) and change it with `set`, `remove`, atomic `edit(&[Edit])`. Untouched bytes remain exact. `ConformantDocument`
has no mutable access; consume it with `into_inner()` to edit again. Clones,
views and proofs retain their revision.

## Choose the work you need

Core owns document semantics, exact JSON, immutable authoring and operation lookup.
Value evaluation is an explicit companion or a caller-provided implementation.
HTTP discovery acquires interface documents and publishes their immutable bytes.

Rust companions define provider traits and checked callers for invocation, kind
description and source checking. A binding invoker interacts with a realization;
an operation invoker applies document selection and per-value schemas. Kind queries
carry operation-specific support claims and check disclosures. Explicit source
checks return a source-entry claim or a separate rejection, context challenge or
operational failure. Kind implementations supply the meaning of source and binding
data. Applications choose providers, execution permission and acquisition policy.
Normalized schema identity is an optional comparison companion. Source pulling
returns exact additions and reports against an original document, with checked
relationships and prospective conformance. Applying changes remains explicit.

Start with the crate guide for
[core](crates/openbindings/README.md),
[value evaluation](crates/openbindings-json-schema-evaluator/README.md),
[evaluator qualification](crates/openbindings-schema-evaluator-test-support/README.md),
[HTTP discovery](crates/openbindings-http-discovery/README.md),
[shared context](crates/openbindings-context/README.md),
[schema comparison](crates/openbindings-schema-comparison/README.md),
[kind description and source checking](crates/openbindings-kind-description/README.md),
[source pulling](crates/openbindings-source-pull/README.md) or
[invocation](crates/openbindings-invocation/README.md).

## Limits and compatibility

Parsing, document conformance and value validation are separate steps. A prepared
schema can still return no-verdict for a value. Core never fetches schema
references. Document validation works without an operation value evaluator;
[the evaluator guide](docs/schema-evaluators.md) explains that boundary.

Rust requires 1.88. The 256 KiB document stack regression is qualified on the
development toolchain; older supported compilers may need larger debug stacks.
TypeScript owners require explicit disposal for timely release. Long synchronous
Wasm work belongs in a Worker when the caller needs responsive cancellation.
The Rust role APIs are provisional. TypeScript does not expose these APIs.

Read [capabilities and limits](CAPABILITIES.md), [host support](docs/support.md),
[performance measurements](docs/performance-boundaries.md) and the
[API reference](docs/api-reference.md). This SDK applies OpenBindings 0.2.
Applied specification revision: `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`.
Package and specification versions are independent.

The [current 0.2 alignment checkpoint](docs/alignment-checkpoint.md) records
implemented boundaries, independent reviews and remaining qualification gates.

Repository build and verification instructions are in
[the maintainer guide](docs/maintainers.md).
