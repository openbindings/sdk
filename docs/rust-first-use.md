# First useful result in Rust

Start with the optional evaluator companion's [first-use example](../crates/openbindings-json-schema-evaluator/examples/first_use.rs). It diagnoses a mistaken document member, corrects the original source, establishes conformance, reads operation metadata, resolves an alias, prepares input, and handles validation outcomes. The separate [replacement example](../crates/openbindings-json-schema-evaluator/examples/replacement.rs) adds explicit resources and retained work.

These packages are unpublished. Use an exact checkout or locally built archives as described in [source replay](REPLAY.md); the package version is independent of the document's `openbindings` version. Rust requires no runtime initialization. A consuming package selects its evaluator explicitly:

```toml
[dependencies]
openbindings = "=0.2.0-alpha.1"
openbindings-json-schema-evaluator = "=0.2.0-alpha.1"
serde = { version = "1.0.229", features = ["derive"] }
```

Until publication, patch those version dependencies and their unpublished siblings to extracted Cargo archives. The companion archive includes both examples and its README; it needs no HTTP client, async executor or evaluator qualification kit to run them. From a source checkout or the companion's extracted package with sibling patches configured:

```sh
cargo run -p openbindings-json-schema-evaluator --example first_use
cargo run -p openbindings-json-schema-evaluator --example replacement
```

## Load, inspect and prepare

`ParsedDocument::parse(bytes)` preserves exact JSON and source evidence. Parsing does not prove conformance. `document.assess()` returns a report for the supported specification line or a separate unsupported-version refusal. A report can be conformant, nonconformant or undetermined. Only `assessment.validated()` yields a conformant document owner.

The first-use example requires conformance before continuing and prints findings when that policy is not met. For editor-style inspection you may choose to keep a parsed document and its findings instead. Creating a value-contract context does not silently establish whole-document conformance.

`document.resolve_operation(name)` accepts either a primary name or an alias. Match `Found`, `Missing` and `Ambiguous`; ambiguity retains candidate primary keys. A found operation gives you `key()`, `description()`, `aliases()`, `bindings()` and its exact `value()`.

There is a deliberate difference between typed metadata and exact schema access. `description()` and `aliases()` interpret strings and can return located errors. `input()` and `output()` return `Option<JsonRef>` with no `?`: absence is `None`, while a present null or false stays an exact value. A present field has not yet been established as a supported contract.

Keep the returned conformance proof, select `DefaultEvaluator` from the optional companion, then call `proof.parsed().value_contracts(Arc::new(DefaultEvaluator::new()), resources)`. `ResourceSet::default()` supplies no external resources. The SDK never fetches a missing reference.

`context.prepare(name, Side::Input)` has these setup outcomes:

| Outcome | Application decision |
| --- | --- |
| `Ready(input)` | Retain the owner and validate admitted values. |
| `NoContract` | Decide whether the operation is usable without that contract. |
| `OperationMissing` | Correct the selected name or choose another operation. |
| `OperationAmbiguous { candidates }` | Resolve the document's repeated identifier occurrences. |
| `NoVerdict { detail }` | Inspect the reason/code/location; correct resources or capability/work conditions as appropriate. |

Preparation refusal is not an input failure and does not prove that the schema has no meaning. Different applications may accept different setup states; the examples require a ready input.

## Diagnose and correct a document

For example, this editor draft uses `inputSchema` where the normative operation member is `input`. Parsing succeeds; assessment locates the unexpected member:

```rust
use openbindings::{ContractPreparation, ParsedDocument, ResourceSet, Side};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = r#"{"openbindings":"0.2.0","operations":{"lookup":{"inputSchema":{"type":"integer"}}}}"#;
    let document = ParsedDocument::parse(text)?;
    let assessment = document.assess()?;
    for finding in &assessment.report().findings {
        // Debug quotes and escapes the pointer, including any control characters.
        println!("{}/{} ({:?}) at {:?}: {}",
            finding.rule, finding.code, finding.status, finding.location, finding.message);
    }
    if assessment.report().findings_truncated {
        println!("More findings were omitted; rule evidence remains available.");
    }
    // Correct this known fixture's original source, not a normalized JSON value.
    let corrected = text.replace("\"inputSchema\":", "\"input\":");
    let corrected = ParsedDocument::parse(corrected)?;
    let assessment = corrected.assess()?;
    let Some(proof) = assessment.validated() else {
        return Err(format!("document conformance: {:?}", assessment.report().conclusion).into());
    };
    let context = proof.parsed().value_contracts(
        Arc::new(DefaultEvaluator::new()), ResourceSet::default())?;
    match context.prepare("lookup", Side::Input) {
        ContractPreparation::Ready(_input) => { /* retain input for value validation */ }
        other => return Err(format!("input setup was refused: {other:?}").into()),
    }
    Ok(())
}
```

The `OBI-02` / `schema-mismatch` finding points to `/operations/lookup/inputSchema` at the original key token and says `this member is not permitted here; extension member names begin with x-`. The caller chooses the intended correction; the SDK does not guess a replacement name. Direct type failures explain the expected type. Missing-field messages identify the fixed-schema field and locate its containing object; no source byte offset is invented for a nonexistent property. A `name-grammar` finding explains the permitted ASCII spelling and first character. Complex schema failures keep a general explanation instead of presenting one alternative as the only repair. Messages are bounded guidance, not stable machine identifiers; use rule, code, evidence and location fields for logic. These fixed-schema messages do not echo rejected values or unexpected member names.

Locations use original JSON Pointers, zero-based UTF-8 byte offsets, and one-based lines and byte columns. Preserve the original document bytes when presenting them. These are not character indexes or JavaScript UTF-16 editor selection offsets: an editor must convert coordinates against the same source bytes before selecting text. Quote or escape pointers when displaying them, and render them as text rather than HTML. Correcting text creates a new snapshot; it does not mutate retained views of the old one. A truncated findings list still accompanies the complete rule evidence map.

## Admit a value and interpret the result

For ordinary Rust data, use `JsonValue::from_serializable(&value)` and handle a possible conversion refusal before validation. Its checked Serde profile retains exact `u64`/`i128`/`u128` values, rejects non-finite floats and unsupported representations, and bounds emitted JSON. For exact text or bytes, use `JsonValue::parse`. Do not route already exact `JsonValue` or Serde `RawValue` wrappers through ordinary conversion. An ordinary float cannot recover digits already rounded by application code.

Retain one ready contract and call `input.validate(&value)` repeatedly:

| Outcome | Meaning |
| --- | --- |
| `Satisfies` | The input satisfies the contract. |
| `Fails { problems, problems_complete }` | A failure is established; selected diagnostics may be incomplete. |
| `NoVerdict { detail }` | The evaluator did not establish a verdict; this is neither success nor failure. |

Failure problems identify instance pointers and original schema locations when available. `problems_complete` describes the selected diagnostic pass, not every logically redundant failed keyword. The first-use example validates an ordinary struct containing `9007199254740993`, then an adjacent smaller exact integer to show a failure without losing digits.

## Retained work and replacement

The replacement example owns an `active: PreparedContract` slot. Its `candidate` helper parses a prospective document, establishes conformance, constructs immutable resources, and requires a ready input declaring `ResourceCompleteness::Complete`. Only a successful result is assigned to `active`. The helper retains distinct parse, version, conformance, interpretation and preparation failures with their diagnostics.

A qualified missing-resource contract can be ready and declare Incomplete. The
example's candidate helper checks borrowed completeness evidence before accepting it,
returns a distinct missing-resources error, and keeps the active owner. Undeclared
custom evaluators require caller policy. A partial preview can deliberately retain
an Incomplete owner instead. Complete does not guarantee decidability: budgets,
cancellation and unsupported instances can still return no verdict. Recovery creates
a new context; it cannot update an old owner's evidence.

The old job clones the ready owner before the application changes the slot. It can still use its original document/resource context after replacement, even when the new context supplies a different schema at exactly the same URI. A channel makes the example's scheduling deterministic; it does not claim to interrupt an evaluation already executing. Applications can use their own threads, queues or other scheduling policy.

Contexts retain four most-recent preparation entries by default. A separate fixture block in `main` uses `ValueContractOptions { cache_capacity: 1 }` to make its second preparation evict its first cache entry; zero disables caching. The reusable `candidate` helper uses the conformance proof and requires only the requested input contract, with no assumption that an output is present. Explicitly retained owners survive eviction and context drop. Cache capacity limits entries, not bytes. Use Rust scopes and `drop` to release obsolete owners; a released owner does not imply lower allocator RSS.

`prepare_with_control` and `validate_with_control` accept `WorkControl`. Clones share a cancellation state, and cancellation is permanent for that state. Create a fresh control for an independent retry. Cancelled preparation returns no ready owner; cancellation and transient evaluator failure do not poison healthy subsequent preparation on the context. A cancelled validation yields no-verdict and leaves the ready contract usable.

Cancellation is cooperative, not a hard deadline. The example exercises cancellation before entry and recovery afterward. Invalid and cancelled candidates leave the existing active owner intact; the application chooses whether and when to install a later healthy replacement. No SDK-managed scheduler or active-document service is needed.

### Interpretation and diagnostic compatibility

Creating a value-contract context checks the entire operation namespace. A malformed
operation or alias returns its specific `InterpretationError` and source location
before selection, even when another operation would otherwise be usable.
`resolve_operation` checks the same namespace. Unrelated metadata may remain
nonconformant for draft evaluation; applications requiring a normative document
must establish conformance before accepting it.

`ValueOutcome::Fails` is the established failure outcome. `NoVerdictReason` and
`InterpretationError` are non-exhaustive cause families: external matches need a
wildcard for future causes. Semantic outcome enums remain closed, and public
diagnostic structs remain constructible by evaluator implementations.

Default diagnostic messages are advisory and bounded. Reference/resource prose
omits source identifiers; value-failure prose omits instance data and user-schema
enum/member strings. Type guidance uses only fixed JSON type names. Structured
locations, reference spellings and explicitly accessed source fields can still
contain source text; apply your application's disclosure policy before logging
them. Custom evaluator messages are supplied by their authors.


Optional default-evaluator schema facts use
`DefaultEvaluator::new().with_schema_details(true)` (TS
`contracts({ includeSchemaDetails: true })`). They retain original source snapshots
and share the diagnostic string-byte budget. `ValueProblemDetails::Truncated`
sets result completeness false; absent details mean disabled or unavailable.
Exact bounds and enum choices remain JSON token strings. Existing Rust evaluator
implementations constructing `ValueProblem` must add `details: None` to preserve
their previous output. The enum is non-exhaustive; match future variants safely.
See the [evaluator resource contract](../crates/openbindings-json-schema-evaluator/README.md).
