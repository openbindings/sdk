# OpenBindings core

Parse, validate, inspect and author OpenBindings interface documents in Rust.
Exact JSON values preserve numeric tokens, source locations and the distinction
between an absent field and JSON null.

```rust
use openbindings::{Conformance, Document};

let document = Document::parse(
    br#"{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"]}}}"#,
)?;
let validation = document.validate()?;
assert_eq!(validation.report().conclusion, Conformance::Conformant);
let conformant = validation.conformant().ok_or("document is not conformant")?;
let operation = conformant.resolve_operation("find").ok_or("operation is missing")?;
assert_eq!(operation.key(), "lookup");
# Ok::<(), Box<dyn std::error::Error>>(())
```

Parsing retains a snapshot for inspection, including invalid editor drafts.
Validation checks all document rules and returns evidence as met, violated or
inconclusive where applicable. Only established conformance produces a
`Conformant<Document>`, with infallible typed field reads and operations that hand out value contracts.
An unsupported specification version is a separate refusal.

Create an OBI with `Document::new()` or parse exact JSON with `Document::parse`.
Edit that same type through `&mut self`: use `set_description`, whole-part setters
such as `set_operation(key, &operation)`, or RFC 6901 `set` and `remove`.
`edit(&[Edit])` commits one atomic batch with one splice and one parse. Sibling
insertions coalesce in batch order; untouched bytes remain exact. The byte budget
is checked before allocating replacement bytes. Final parsing enforces the default
JSON limits. A failed batch leaves the document unchanged.

A `Conformant<Document>` has infallible reads and no mutable access. Use
`as_document()` for plain fallible reads, or consume it with `into_inner()` to edit
again. Clones, views, validation reports and proofs retain their own revision;
editing another owner cannot change them. Positions refer to each revision's
current bytes, available from `to_bytes()`. Duplicate-member input remains
inspectable but is refused by typed reads, editing and conformance proof.

The [Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
shows value validation and an exact opaque-member edit.

## Value contracts and limits

Document validation works without a caller-supplied schema evaluator. Its fixed
normative schema checks are part of core. Operation value validation requires an
explicit `SchemaEvaluator`, including for `true`, `false` and empty schemas.
Select the optional `openbindings-json-schema-evaluator` or implement the trait.
A conformant operation's `input()` and `output()` return its `ValueContract`s, or
`None` when it declares none. `prepare(&evaluator)` borrows the evaluator and
supplies no resources; `prepare_with(&evaluator, &resources)` adds an immutable
`ResourceSet`. Core does not depend on the companion.
The [evaluator integration guide](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md)
explains the no-evaluator behavior and custom implementations.

Prepare a contract, retain the `PreparedContract`, and handle satisfies, fails,
undefined and no-verdict for each value. Preparation returns
`Result<PreparedContract, NoVerdict>`, so `?` propagates a refusal with its
payload. Use `PreparedContract::validate_json` for exact JSON text or bytes; only
admission failure returns `Err`. Retain a `JsonValue` and use `validate` for
repeated checks or custom admission limits. Undefined means the value-contract
rules leave whether the value satisfies its contract undefined; no-verdict means
the evaluator could not answer. Each preparation compiles independently; cloning a
prepared contract shares its compiled state. A `ResourceSet` indexes its resource
identities once. For evaluators that request the core projection, a resolution
space is assembled once per live document revision and set, and released when
either is dropped.

Core performs no resource acquisition, binding selection or invocation. Source
and binding content remain opaque. Kind implementations interpret them outside
core. Document conformance does not establish that a binding works or that an
example satisfies its value contract.

Default JSON admission allows 64 MiB, 10,000 container levels and 1,000,000 nodes.
Checked ordinary Serde conversion has a 128-container ceiling. Exact parsing is
the route for existing JSON tokens. Findings retain at most 4,096 entries and
8 MiB of pointers; truncation leaves rule evidence intact. Use codes and evidence
for program logic. Messages are advisory; locations refer to the assessed revision's UTF-8 bytes.

This unpublished package requires Rust 1.88. The 256 KiB document stack regression
is qualified on the development toolchain, not on every supported compiler.
See [support](https://github.com/openbindings/sdk/blob/main/docs/support.md) and
[performance](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md).

## Reference

Use the crate's type and method documentation for ownership, error and limit
contracts. The [API reference guide](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md)
explains how to build the full reference locally.
