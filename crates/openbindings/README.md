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
assert!(round_trip.resolve_operation("lookup")?.is_some());
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
transient failures are retryable. Handle every `ValueOutcome` variant: satisfies,
mismatch, no-contract, operation-missing and no-verdict. Preparation refusal is not
proof of semantic undefinedness. `openbindings-schema-evaluator-test-support`
qualifies custom adapters; `openbindings-http-discovery` supplies optional HTTP policy.

Source locations use original JSON Pointers, zero-based UTF-8 byte offsets and
one-based lines/byte columns. Bounded diagnostic output declares truncation.
Resources are immutable and caller supplied; acquisition URLs do not change schema
bases. Anonymous document and dynamic scope remain part of interpretation.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Applied
specification revision: 2f7d754dc2da374058cd517064c17e50f7d95d99. Package publication
and application cutover are separate actions.
