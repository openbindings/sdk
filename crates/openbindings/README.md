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

`ParsedDocument` preserves input and duplicate evidence. `DocumentAssessment`
reports all 13 rules; only established conformance produces `ValidatedDocument`.
Names/aliases are indexed. Inspection does not perform preference selection.
Version policy supports stable 0.2.x with an explicit applied specification revision.

`JsonValue` preserves exact numeric tokens, unknown members, null/absence and
source locations. Use its text/bytes for values such as 9007199254740993. Typed
`DocumentBuilder` fields model normative objects; `additional_fields` retain exact
unknown members but cannot shadow typed fields. Building is separate from checking
conformance. JSON input defaults to 64 MiB, 10,000 containers and 1,000,000 nodes.

Operation validation uses explicit `ResourceSet`, `SchemaEvaluator` and immutable
`ValueContracts`. Select the separate `openbindings-json-schema-evaluator` companion
or implement the trait. Prepared contracts retain required owners after document
or context drop. The default context cache has four entries, configurable through
`ValueContractOptions`; retained contracts survive eviction. Cancellation and
transient failures are retryable. Handle every `ContractPreparation` branch: ready, no-contract, operation-missing,
operation-ambiguous and no-verdict. Only ready contracts expose value validation;
`ValueOutcome` is satisfies, mismatch or no-verdict. Preparation refusal is not
proof of semantic undefinedness. `openbindings-schema-evaluator-test-support`
qualifies custom adapters; `openbindings-http-discovery` supplies optional HTTP policy.

Source locations use original JSON Pointers, zero-based UTF-8 byte offsets and
one-based lines/byte columns. Bounded diagnostic output declares truncation.
Resources are immutable and caller supplied; acquisition URLs do not change schema
bases. Anonymous document and dynamic scope remain part of interpretation.

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
separate domain. See the [migration notes](../../docs/migration.md).
