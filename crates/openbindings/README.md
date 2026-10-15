# OpenBindings core

Parse, validate, inspect and author OpenBindings interface documents in Rust.
Exact JSON values preserve numeric tokens, source locations and the distinction
between an absent field and JSON null.

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

Parsing retains a snapshot for inspection, including invalid editor drafts.
Validation checks all document rules and returns evidence as met, violated or
inconclusive where applicable. Only established conformance produces a
`ValidatedDocument`, with infallible typed field reads and access to value contracts.
An unsupported specification version is a separate refusal.

To author a document, create `Document::new()`, populate its typed fields and call
`build()`. To edit a parsed document, call `to_authoring()`, change the draft, then
build and validate a new snapshot. The
[Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
shows value validation and an exact opaque-object edit.

## Value contracts and limits

Document validation works without a caller-supplied schema evaluator. Its fixed
normative schema checks are part of core. Operation value validation requires an
explicit `SchemaEvaluator`, including for `true`, `false` and empty schemas.
Select the optional `openbindings-json-schema-evaluator` or implement the trait.
The companion's `value_contracts(&validated)` function supplies its default
budgets, coarse diagnostics and no extra resources. For shared or configured
evaluators and supplied resources, pass an `Arc<dyn SchemaEvaluator>` and an
immutable `ResourceSet` to `ValidatedDocument::value_contracts`. Core does not
depend on the companion.
The [evaluator integration guide](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md)
explains the no-evaluator behavior and custom implementations.

Prepare a direction, retain a ready `ValueContract`, and handle satisfies, fails, undefined
and no-verdict for each value. Use `ValueContractPreparation::into_result()` to
propagate a non-ready reason with `?`, preserving its payload. Use
`ValueContract::validate_json` for exact JSON text or bytes; only admission failure
returns `Err`. Retain a `JsonValue` and use `validate` for repeated checks or custom
admission limits. Preparation can also report a missing operation,
an absent value contract or no-verdict. Undefined means the value-contract rules
leave whether the value satisfies its contract undefined; no-verdict means the
evaluator could not answer. Each preparation compiles independently;
cloning a ready contract reuses its retained document and resources.

Core performs no resource acquisition, binding selection or invocation. Source
and binding content remain opaque. Kind implementations interpret them outside
core. Document conformance does not establish that a binding works or that an
example satisfies its value contract.

Default JSON admission allows 64 MiB, 10,000 container levels and 1,000,000 nodes.
Checked ordinary Serde conversion has a 128-container ceiling. Exact parsing is
the route for existing JSON tokens. Findings retain at most 4,096 entries and
8 MiB of pointers; truncation leaves rule evidence intact. Use codes and evidence
for program logic. Messages are advisory; locations refer to original UTF-8 bytes.

This unpublished package requires Rust 1.88. The 256 KiB document stack regression
is qualified on the development toolchain, not on every supported compiler.
See [support](https://github.com/openbindings/sdk/blob/main/docs/support.md) and
[performance](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md).

## Reference

Use the crate's type and method documentation for ownership, error and limit
contracts. The [API reference guide](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md)
explains how to build the full reference locally.
