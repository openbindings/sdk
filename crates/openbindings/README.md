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

In a Rust Worker service, validate documents and prepare schemas inside request
handling, following the [Worker service rule in the support table](https://github.com/openbindings/sdk/blob/main/docs/support.md).

To author, create an OBI with `Document::new()` or parse exact JSON with
`Document::parse`, which retains a snapshot for inspection, including invalid
editor drafts, and refuses text that is not JSON as an `InputError`;
`Document::validate` then assesses that snapshot under its retained limits.
Default parsing and direct validation use the same `JsonLimits::default()`. Edits borrow the original and return a new immutable snapshot: use RFC 6901
`set` and `remove` with exact JSON values. Set a complete namespace when absent;
editing does not fabricate intermediate containers.
`edit(&[Edit])` commits one atomic batch with one splice and one parse. Sibling
insertions coalesce in batch order; untouched bytes remain exact. The byte budget
is checked before allocating replacement bytes. Final parsing enforces the JSON limits retained by the document. Both successful and failed batches leave the original document unchanged. Retain the returned document to use an edit.

A `ConformantDocument` has infallible reads and no mutable access. Use
`document()` for exact draft reads, or consume it with `into_inner()` to keep the underlying snapshot. Clones, views, validation reports and proofs retain their own revision;
editing another owner cannot change them. Positions refer to each revision's
current bytes, available from `as_bytes()`. Duplicate-member input remains
inspectable but is refused by editing and conformance proof. Canonical domain entries are acquired
only from a `ConformantDocument`.

The [Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
shows value validation and an exact opaque-member edit.

## Value schemas and limits

Document validation works without a caller-supplied schema evaluator. Its fixed
normative schema checks are part of core. Operation value validation requires an
explicit `SchemaEvaluator`, including for `true`, `false` and empty schemas.
Select the optional `openbindings-json-schema-evaluator` or implement the trait.
A conformant operation's `input()` and `output()` return its `Schema`s, or
`None` when it declares none. Named schemas come from `ConformantDocument::schema`.
`prepare(&evaluator, &external_schemas)` borrows explicit inputs
and returns `Result<PreparedSchema<E::Validator>, PreparationFailure>` with default
preparation limits and no cancellation. Use `prepare_with_control` for explicit
limits or cancellation. Core does
not depend on the evaluator companion. Original schema inventory and resolution
are available through `SchemaEnvironment`; evaluator projections remain private.

Retain a prepared schema and handle satisfies, fails, undefined and no-verdict for
each value. Preparation failure judges no value. Parse exact JSON text or bytes
with `JsonValue::parse` or `parse_with_limits`, then call `evaluate(&value)`.
Admission failure is separate from the four value outcomes. `explain` requests
diagnostics for the same verdict. Undefined establishes semantic dependence on a
known undefined result; no-verdict means the implementation could not answer.
Preparation work is optional and off by default (`max_sdk_work_units: None`);
a time bound is the caller's choice through `WorkControl` cancellation or
`Some(work_units)`. The 512 MiB reservation allowance bounds conservative
estimates of memory held at once. Dropped scratch is released and retained
indexes/programs stay charged. See the dated
[host-specific 250 ms suggestion](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md#limits-principle-november-2026).
Each preparation is independent. Ownership and thread-safety follow the concrete
validator; the default evaluator shares compiled state when cloned.

For input buffers containing several values, `JsonValue::parse_prefix(input)`
returns the first exact value and its exclusive token-end offset.
`JsonPrefixError::Incomplete` means the boundary is unknown; `Invalid(InputError)`
reports an admission failure. At known EOF use `JsonValue::parse_with_limits` on
an incomplete remainder to accept a final number or obtain its ordinary refusal.
After `Incomplete`, append at least the current buffer length before retrying,
unless EOF or a limit is reached first, to keep cumulative parsing work linear.
The host owns separators and I/O. The
[sequence example](https://github.com/openbindings/sdk/blob/main/crates/openbindings/examples/json_sequence.rs)
implements that geometric rule with bounded reads and whitespace separators.
`parse_prefix_with_limits` applies the same byte, depth and node allowances as
`parse_with_limits` through the first value end. Calls inspect only that value
and constant lookahead, preserving invalid UTF-8 in later buffered bytes.

Core performs no resource acquisition, binding selection or invocation. Source
and binding data remain opaque. Kind implementations interpret them outside
core. Document conformance does not establish that a binding works or that an
example satisfies its schema.

Default JSON admission allows 32 MiB, 10,000 container levels and 1,000,000 nodes:
`Document::parse`, `Document::parse_owned` and `JsonValue::parse` apply all three.
`validate_document` and `validate_document_owned` apply all three defaults too.
Past the depth cap, whole-document validation is undetermined with `depth-limit`.
A host that needs other limits passes `JsonLimits` to
`validate_document_with_limits`, `Document::parse_with_limits` or
`JsonValue::parse_with_limits`, raising `max_depth` for deeper documents. A document keeps the limits it was admitted under
(`Document::limits`) for its edits.
Checked ordinary Serde conversion has a 128-container ceiling. Exact parsing is
the route for existing JSON tokens. Findings retain at most 4,096 entries and
8 MiB of pointers. OBI-02 also carries `keyword`, `schema` (a `SchemaOrigin` in
`https://openbindings.com/schema/openbindings-0.2.0.json`) and optional `details`
using `ValueProblemDetails`. Required facts name one missing member per finding;
type facts list expected types. Disallowed members use their source pointer and
`additionalProperties`, without copying their names or values into details.
Failed `anyOf` and `oneOf` report at the applicator, without branch facts, matching
value explanations. The pinned document schema uses `anyOf`, not `oneOf`.
`pattern` and `uniqueItems` also have no detail variant. Numeric and size bounds
use exact schema token strings; enum facts contain the complete choices.
The pinned 0.2 schema's largest serialized keyword/schema/details tuple is 199
bytes. An exhaustive schema test guards this property; no runtime fact-size cap
is needed. Finding-count and source-pointer truncation leave rule evidence intact. Use codes and evidence
for program logic. Messages are advisory; locations refer to the assessed revision's UTF-8 bytes.

This unpublished package requires Rust 1.88. The 256 KiB document stack regression
is qualified on the development toolchain, not on every supported compiler.
See [support](https://github.com/openbindings/sdk/blob/main/docs/support.md) and
[performance](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md).

## Reference

Use the crate's type and method documentation for ownership, error and limit
interfaces. The [API reference guide](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md)
explains how to build the full reference locally.

## Document finding codes

`Finding.code` uses the stable identifiers below in Rust and TypeScript document
conformance reports. Read it together with `rule` and `status`; one code can
apply to several rules. These are document findings, separate from input/edit
errors and schema-instance value problems. Input limits emit inconclusive findings
for all rules at the parser offset with no pointer. Their messages explain the
reached limit without API advice. To admit more, use `validate_document_with_limits`
with a larger `JsonLimits::max_bytes`, `max_depth` or `max_nodes`, respectively.
A fixed-schema check that cannot finish emits one for its rule.
The other codes describe violations. Unsupported declared versions are separate
refusals, not finding codes.

| Code | Rule | Meaning |
| --- | --- | --- |
| `bom` | OBI-01 | JSON text begins with a forbidden byte-order mark. |
| `colon` | OBI-01 | An object member name is not followed by a colon. |
| `comma-or-close` | OBI-01 | A container needs a comma or its closing delimiter. |
| `escape` | OBI-01 | A string contains an invalid JSON escape. |
| `exponent` | OBI-01 | A number exponent lacks a required digit. |
| `fraction` | OBI-01 | A decimal point lacks a following digit. |
| `literal` | OBI-01 | A token does not spell `true`, `false` or `null`. |
| `number` | OBI-01 | A number token violates JSON number syntax. |
| `object-key` | OBI-01 | An object needs a quoted member name. |
| `string-control` | OBI-01 | A string contains an unescaped control character. |
| `trailing-token` | OBI-01 | Content follows the complete JSON value. |
| `unicode-escape` | OBI-01 | A Unicode escape lacks four hexadecimal digits. |
| `unterminated-string` | OBI-01 | A string lacks its closing quote. |
| `utf8` | OBI-01 | Source bytes are not valid UTF-8. |
| `value` | OBI-01 | A JSON value is required at this position. |
| `duplicate-member` | OBI-01 | An object repeats a decoded member name. |
| `unpaired-surrogate` | OBI-01 | A member name or string contains an unpaired UTF-16 surrogate. |
| `byte-limit` | OBI-01 through OBI-14 | The byte admission limit prevents assessment, at the parser offset. |
| `depth-limit` | OBI-01 through OBI-14 | The container nesting admission limit prevents assessment, at the parser offset. |
| `node-limit` | OBI-01 through OBI-14 | The JSON node admission limit prevents assessment, at the parser offset. |
| `schema-mismatch` | OBI-02, OBI-10 | The document or a contained schema violates its fixed normative schema. |
| `fixed-schema-limit-or-failure` | OBI-02, OBI-10 | A fixed normative schema check cannot establish a result. |
| `semver` | OBI-03 | The `openbindings` member is not a SemVer 2.0.0 string. |
| `missing-version` | OBI-03 | The required `openbindings` member is absent. |
| `name-grammar` | OBI-04 | An entry key, operation alias or example key violates the name grammar. |
| `duplicate-operation-name` | OBI-05 | An operation alias duplicates a primary key or another alias. |
| `unknown-operation` | OBI-06, OBI-08 | A binding or dependency operation reference does not name a primary operation key. |
| `unknown-source` | OBI-07 | A binding source reference does not name a source key. |
| `schema-dialect` | OBI-09 | A contained schema declares a dialect other than JSON Schema 2020-12. |
| `absolute-schema-id` | OBI-11 | A schema `$id` at an OBI position is not an absolute URI. |
| `reference-type` | OBI-11 | A document-resource schema reference is not a string. |
| `reference-form` | OBI-11 | A document-resource reference is not a valid absolute or same-document URI-reference. |
| `reference-target` | OBI-12 | A reference into the document or a contained resource identifies no contained schema. |
| `duplicate-anchor` | OBI-13 | A plain anchor name is declared repeatedly in the document resource. |
| `duplicate-schema-id` | OBI-13 | Contained schemas declare the same compared identifier. |
| `document-dynamic-anchor` | OBI-14 | A dynamic anchor appears outside an identified schema resource. |

`Document::retained_bytes()` estimates the heap storage of a snapshot at the time
of the call, including original source, parsed nodes and built indexes/caches.
Clones and proofs share it: charge a snapshot once, reaching it through a proof's
`document()`. It reads existing capacities on demand, adds no parse/validate work,
and excludes allocator overhead, separately owned external resources and validators,
process-wide packaged schemas, and Wasm memory growth. Namespace/schema lookups
and location requests can grow caches; see its rustdoc for growth bounds.

`InputError::line()` and `byte_column()` report one-based lines and UTF-8 byte
columns, computed once on failure; nothing changes on success.
