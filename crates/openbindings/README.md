# OpenBindings core

Exact document semantics and authoring for OpenBindings 0.2. Core performs no
HTTP/file acquisition, kind execution, binding selection or operation invocation.

```rust
use openbindings::{DocumentBuilder, Operation, ParsedDocument};
let mut draft = DocumentBuilder::new();
draft.operations.insert("lookup".into(), Operation::default());
let document = draft.build()?;
let assessment = document.assess()?;
let conformant = assessment.validated().ok_or("document is not conformant")?;
let round_trip = ParsedDocument::parse(conformant.original_bytes())?;
assert!(matches!(round_trip.resolve_operation("lookup")?, openbindings::OperationSelection::Found(_)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

For the complete document-to-input flow, select the optional
[`openbindings-json-schema-evaluator`](https://github.com/openbindings/sdk/tree/main/crates/openbindings-json-schema-evaluator)
companion. Its package ships `examples/first_use.rs` and `examples/replacement.rs`;
they cover metadata, aliases, ordinary/exact input, readiness, retained work and
failed replacement without an HTTP client or async runtime. Start with the
[Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md).
Rust has no runtime initialization step. The example above uses only core and
establishes document conformance; evaluating operation values requires that explicit
companion or your own evaluator.

`ParsedDocument` preserves input and duplicate evidence. `DocumentAssessment`
reports all 13 rules; only established conformance produces `ValidatedDocument`.
Names/aliases are indexed. Inspection does not perform preference selection.
Version policy supports stable 0.2.x with an explicit applied specification revision.

`JsonValue` preserves exact numeric tokens, unknown members, null/absence and
source locations. Use its text/bytes for values such as 9007199254740993. Typed
`DocumentBuilder` fields model normative objects; `additional_fields` retain exact
unknown members but cannot shadow typed fields. Building is separate from checking
conformance. JSON input defaults to 64 MiB, 10,000 containers and 1,000,000 nodes.

To edit an existing document, call `ParsedDocument::to_authoring()`, update the
builder, then `build()` and assess the new snapshot. Typed fields are re-encoded:
integer preferences such as `1.0` or `1e0` become `1`, and `-0` becomes `0`.
Opaque exact values keep their numeric tokens. Operation renaming leaves reference
updates to the caller.

The packaged [exact object editing example](examples/exact_edit.rs) replaces one
opaque member using public traversal, exact `JsonValue` children and their Serde
representation. It uses the same builder path and verifies the result after the
input owners are dropped. Its generic application helper requires unique names
throughout the subtree and UTF-8-representable member names; it refuses absent
members. It serializes/parses the entire changed object and orders its immediate
names in a `BTreeMap`. Formatting, ordering and member-name escaping may change.
This helper is example code, not another SDK export or a general JSON Patch API.

Operation validation uses explicit `ResourceSet`, `SchemaEvaluator` and immutable
`ValueContracts`. Select the separate `openbindings-json-schema-evaluator` companion
or implement the trait. Prepared contracts retain required owners after document
or context drop. The default context cache has four entries, configurable through
`ValueContractOptions`; retained contracts survive eviction. Cancellation and
transient failures are retryable. Handle every `ContractPreparation` branch: ready, no-contract, operation-missing,
operation-ambiguous and no-verdict. Only ready contracts expose value validation;
`ValueOutcome` is satisfies, fails or no-verdict. Preparation refusal is not
proof of semantic undefinedness. `openbindings-schema-evaluator-test-support`
qualifies custom adapters; `openbindings-http-discovery` supplies optional HTTP policy.

Source locations use original JSON Pointers, zero-based UTF-8 byte offsets and
one-based lines/byte columns. Unexpected normative fields identify original key
tokens and explain the `x-` extension convention. Reports retain at most 4,096
findings; expanded unexpected-field pointers also share an 8 MiB byte budget.
Omitted findings set `findings_truncated`; all 13 rule evidence states remain
available. Expanded findings can displace later ones at the count cap. Messages
are advisory; use rule/code fields for classification and quote pointers when
rendering caller-controlled text.
Resources are immutable and caller supplied; acquisition URLs do not change schema
bases. Anonymous document and dynamic scope remain part of interpretation.

The bundled resolver and default evaluator give a unique contained declared ID
priority over supplied names; packaged standards are the final fallback. Supplied
retrieval aliases and declared IDs compete equally when no contained ID owns the
name. A reached carrier must itself own its canonical ID: aliases, pointers and
direct applicators into a competing carrier conservatively refuse preparation.
Unused conflicts do not poison other contracts, and an independently named nested
resource retains its original base and dialect. No schema-body comparison, alias
transfer or missing-fragment fallback occurs. `SchemaRequest::supplied_resources()`
retains the original catalog. `SchemaLocation.resource` records original retrieval
provenance, which need not be the winning lookup association for that URI. Replacing
resources requires a new immutable context; retained contracts keep their inputs.
Name comparison remains exact after RFC resolution, literal-dot removal and empty
fragment removal, without added case folding or percent-encoding equivalence.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Applied
specification revision: 2f7d754dc2da374058cd517064c17e50f7d95d99. Package publication
and application cutover are separate actions.

Operation selection returns `OperationSelection::Found`, `Missing` or `Ambiguous`.
Repeated identifier occurrences are ambiguous even within one operation; candidates
are distinct primary keys in lexical order. Typed inspection reports malformed
operation objects or aliases with `InterpretationError` source locations. Retained
`OperationView` methods expose description, aliases, schemas and binding keys;
`value()` keeps exact inspection available. An absent schema differs from null/false.

For ordinary Rust values, `JsonValue::from_serializable(&value)` and
`from_serializable_with_limits(&value, limits)` accept Serde without requiring
`Clone`, `Send`, `Sync` or `'static` on the input. The checked profile admits finite
numbers including exact i128/u128, strings/chars, unit/None, byte arrays, standard
containers and Serde enums. Map keys must explicitly serialize as strings
(including `collect_str` and transparent string newtypes); duplicates and non-finite
numbers are refused. The pinned `serde_json::Number` protocol is checked as one standalone number, so
ordinary `serde_json::Value` numeric leaves retain their token spelling, including
wide integer and decimal tokens. Exact `JsonValue` and Serde `RawValue` wrappers use the exact
parse/retain lane and are deliberately refused by this converter.

Emitted bytes, nodes and container depth are bounded during conversion. Checked
Serde conversion has a 128-container profile ceiling: effective depth is
`min(limits.max_depth, 128)`, including the default constructor. Iterative exact
parsing keeps its existing 10,000-container default. After a
refusal, admission remains closed even if user serialization ignores an error.
These bounds do not preempt arbitrary user `Serialize`/`Display` code or bound its
allocations. `ValueConversionError` owns a bounded explicit `message()` (4 KiB),
with truncation flags, and a complete emitted JSON pointer (4 KiB maximum, omitted
with a flag when larger). Display and Debug omit arbitrary custom prose.
`AuthoringError` distinguishes field collisions from conformance and exposes native
draft pointers containing `additional_fields`; original `source_location()` is a
separate domain. See the [migration notes](https://github.com/openbindings/sdk/blob/main/docs/migration.md).

Fixed-schema conformance findings explain direct expected types, missing fields,
unexpected normative members and identifier grammar without echoing rejected
values. Unexpected-member findings identify the original key token; required-field
locations identify the containing object. Quote or escape displayed pointers and
label UTF-8 byte coordinates explicitly. Complex failures retain a general
explanation. Message wording is guidance; use rule/code/evidence and original
locations for program logic. The [Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
shows source correction through conformance proof and contract setup.

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](../../docs/api-reference.md).
