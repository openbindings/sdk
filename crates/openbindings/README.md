# OpenBindings core

Parse, validate, inspect and author OpenBindings interface documents in Rust.
Exact JSON values preserve numeric tokens, source locations and the distinction
between an absent field and JSON null.

```rust
use openbindings::{Conformance, validate_document};

let validation = validate_document(
    br#"{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"]}}}"#,
)?;
assert_eq!(validation.report().conclusion, Conformance::Conformant);
let conformant = validation.conformant().ok_or("document is not conformant")?;
let operation = conformant.resolve_operation("find").ok_or("operation is missing")?;
assert_eq!(operation.key(), "lookup");
# Ok::<(), Box<dyn std::error::Error>>(())
```

`validate_document` checks all document rules on the bytes as given and returns
evidence as met, violated or inconclusive where applicable. Text that is not JSON
is an OBI-01 violation in the report, with its location. Only established
conformance produces a `ConformantDocument`, with infallible typed field reads
and operations that hand out schemas. An unsupported specification
version is a separate refusal.

To author, create an OBI with `Document::new()` or parse exact JSON with
`Document::parse`, which retains a snapshot for inspection, including invalid
editor drafts, and refuses text that is not JSON as an `InputError`;
`Document::validate` then returns the same validation. Edits borrow the original and return a new immutable snapshot: use RFC 6901
`set` and `remove` with exact JSON values. Set a complete namespace when absent;
editing does not fabricate intermediate containers.
`edit(&[Edit])` commits one atomic batch with one splice and one parse. Sibling
insertions coalesce in batch order; untouched bytes remain exact. The byte budget
is checked before allocating replacement bytes. Final parsing enforces the default
JSON limits. Both successful and failed batches leave the original document unchanged. Retain the returned document to use an edit.

A `ConformantDocument` has infallible reads and no mutable access. Use
`document()` for exact draft reads, or consume it with `into_inner()` to keep the underlying snapshot. Clones, views, validation reports and proofs retain their own revision;
editing another owner cannot change them. Positions refer to each revision's
current bytes, available from `as_bytes()`. Duplicate-member input remains
inspectable but is refused by editing and conformance proof. Canonical domain entries are acquired
only from a `ConformantDocument`.

The [Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
shows value validation and an exact opaque-member edit.

## Value contracts and limits

Document validation works without a caller-supplied schema evaluator. Its fixed
normative schema checks are part of core. Operation value validation requires an
explicit `SchemaEvaluator`, including for `true`, `false` and empty schemas.
Select the optional `openbindings-json-schema-evaluator` or implement the trait.
A conformant operation's `input()` and `output()` return its `Schema`s, or
`None` when it declares none. Named schemas come from `ConformantDocument::schema`.
`prepare(&evaluator, &external_schemas, &preparation_limits)` borrows explicit inputs
and returns `Result<PreparedSchema<E::Validator>, PreparationFailure>`. Core does
not depend on the evaluator companion. Original schema inventory and resolution
are available through `SchemaEnvironment`; evaluator projections remain private.

Retain a prepared schema and handle satisfies, fails, undefined and no-verdict for
each value. Preparation failure judges no value. `evaluate_json` admits exact JSON
text or bytes; only admission failure is `Err`. Retain a `JsonValue` and call
`evaluate` for repeated checks or custom admission limits. `explain` requests
diagnostics for the same verdict. Undefined establishes semantic dependence on a
known undefined result; no-verdict means the implementation could not answer.
Each preparation is independent. Ownership and thread-safety follow the concrete
validator; the default evaluator shares compiled state when cloned.

Core performs no resource acquisition, binding selection or invocation. Source
and binding data remain opaque. Kind implementations interpret them outside
core. Document conformance does not establish that a binding works or that an
example satisfies its schema.

Default JSON admission allows 32 MiB, 10,000 container levels and 1,000,000 nodes:
`Document::parse`, `Document::parse_owned` and `JsonValue::parse` apply all three.
`validate_document` and `validate_document_owned` apply the same bytes and nodes
at any nesting. A host that needs other limits passes `JsonLimits` to
`validate_document_with_limits`, `Document::parse_with_limits` or
`JsonValue::parse_with_limits`; `JsonLimits::default()` there includes the
10,000-level cap. A document keeps the limits it was admitted under
(`Document::limits`) for its edits.
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
