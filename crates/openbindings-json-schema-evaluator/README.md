# OpenBindings JSON Schema evaluator

This optional companion evaluates OpenBindings operation contracts using an explicit immutable resource context. It supports JSON Schema 2020-12 and exact JSON numbers. It never acquires HTTP or filesystem resources.

Use both `openbindings = "=0.2.0-alpha.1"` and this companion in your Cargo
manifest. Packages are currently unpublished: consume locally built archives with
patches for their unpublished siblings, or use an exact source checkout. Rust needs
no runtime initialization.

```rust
use openbindings::{ContractPreparation, JsonValue, ParsedDocument, ResourceSet, Side, ValueOutcome};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = ParsedDocument::parse(
        br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
    )?;
    let assessment = document.assess()?;
    if assessment.validated().is_none() {
        return Err(format!("document conformance: {:?}; findings: {:?}",
            assessment.report().conclusion, assessment.report().findings).into());
    }
    let context = document.value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())?;
    let input = match context.prepare("lookup", Side::Input) {
        ContractPreparation::Ready(input) => input,
        ContractPreparation::NoContract => return Err("lookup has no input contract".into()),
        ContractPreparation::OperationMissing => return Err("lookup is missing".into()),
        ContractPreparation::OperationAmbiguous { candidates } => {
            return Err(format!("lookup is ambiguous: {candidates:?}").into());
        }
        ContractPreparation::NoVerdict { detail } => {
            return Err(format!("preparation refused ({:?}): {}", detail.reason, detail.message).into());
        }
    };
    let ordinary = JsonValue::from_serializable("item")?;
    match input.validate(&ordinary) {
        ValueOutcome::Satisfies => println!("input satisfies the contract"),
        ValueOutcome::Fails { problems, problems_complete } => {
            println!("fails: {problems:?}; diagnostics complete: {problems_complete}");
        }
        ValueOutcome::NoVerdict { detail } => {
            println!("no verdict ({:?}): {}", detail.reason, detail.message);
        }
    }
    Ok(())
}
```

The shipped [first-use example](examples/first_use.rs) adds metadata, alias selection,
exact wide integers and original failure locations. The separate
[replacement example](examples/replacement.rs) shows caller-owned replacement,
explicit same-URI resources, retained old work, invalid/cancelled candidates and
healthy recovery. Run them from this package or the workspace:

```sh
cargo run -p openbindings-json-schema-evaluator --example first_use
cargo run -p openbindings-json-schema-evaluator --example replacement
```

The extended [Rust guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
explains located malformed-field correction and every setup/result family. Exact
schema access (`operation.input()`) returns an optional value without interpretation;
metadata access such as `description()` can return located errors. Admitting an
ordinary Rust value is a separate fallible step from validating it. Use
`JsonValue::parse` for exact JSON text/bytes. Keep already exact `JsonValue` owners
or retained views directly instead of admitting them again through Serde.


A prepared contract retains its resources after the document/context is dropped. Preparation reuses deterministic results within a context. The context retains four most-recent prepared entries by default; `ValueContractOptions.cache_capacity` controls this count, including zero to disable caching. Caller-retained contracts survive eviction; this is an entry cap, not a byte cap. Concurrent first calls may perform duplicate preparation before one result is retained. Cancellation and transient evaluator failures are not cached. Different contexts never share resource identities or compiled contracts.

`NoVerdict` distinguishes unsupported capabilities, conservative preparation, missing resources, limits and cancellation. A missing reference or invalid pattern in a potentially visited branch may cause a conservative refusal even if another branch would establish validity. Such a refusal does not claim semantic undefinedness. Unicode property-escape *syntax* follows ECMA-262 edition 11; actual property matching is explicitly unsupported and declined only when applied. `format` remains an annotation. Custom/historical dialects are not interpreted as 2020-12.

Regex compilation refusals use `schema-pattern-compilation` with bounded guidance
to inspect `pattern` and `patternProperties`. They do not echo patterns or resource
identifiers. A source location remains absent when the compiler cannot establish
the original resource; other unclassified preparation failures retain the generic
`evaluator-preparation` diagnostic.

Failures report actual instance locations and original schema locations where available. They do not promise every possible failing keyword or Go's diagnostic multiplicity. `problems_complete` means the selected diagnostic pass completed without truncation, not exhaustive traversal of every semantically redundant failure. Diagnostic collection has its own work scope and allocation cap; a confirmed failure remains a failure when its diagnostics are truncated. Messages omit instance values.

`Limits` bounds evaluation work, recursion, regex work, diagnostic output, dependency compilation depth and pattern admission. Parsing, graph preparation and dependency compilation use separate admission bounds. Cancellation is cooperative; dependency compilation/evaluation has bounded regions that are not preempted midway. Browser applications should use a Worker for large synchronous jobs.

The vendored adaptation is an implementation detail. Upgrade it with the recorded patch invariants and the complete evaluator contract suite; a dependency draft label alone is insufficient evidence.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Package publication and application cutover are separate actions.

`WorkControl` clones share permanent cancellation; a fresh control starts a healthy
attempt. Cancelled preparation returns no ready owner, while cancelled validation
leaves an existing contract usable. The application decides whether a candidate is
ready to replace active work and controls its scheduling. Releasing the final Rust
owner is distinct from reducing allocator RSS.

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](../../docs/api-reference.md).
