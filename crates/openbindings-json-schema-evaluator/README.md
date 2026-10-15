# OpenBindings JSON Schema evaluator

Validate values against an operation's input or output value contract. This
optional companion supports JSON Schema 2020-12, exact JSON numbers and explicit
immutable schema resources. Use it alongside `openbindings`.

```rust
use openbindings::{ValueContractPreparation, JsonValue, ParsedDocument, ResourceSet, Direction, ValueOutcome};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = ParsedDocument::parse(
        br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
    )?;
    let validation = document.validate()?;
    if validation.validated().is_none() {
        return Err(format!("document conformance: {:?}; findings: {:?}",
            validation.report().conclusion, validation.report().findings).into());
    }
    let proof = validation.validated().ok_or("document is not conformant")?;
    let context = proof.value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default());
    let input = match context.prepare("lookup", Direction::Input) {
        ValueContractPreparation::Ready(input) => input,
        ValueContractPreparation::NoValueContract => return Err("lookup has no input contract".into()),
        ValueContractPreparation::OperationMissing => return Err("lookup is missing".into()),
        ValueContractPreparation::OperationAmbiguous { candidates } => {
            return Err(format!("lookup is ambiguous: {candidates:?}").into());
        }
        ValueContractPreparation::NoVerdict { detail } => {
            return Err(format!("preparation refused ({:?}): {}", detail.reason, detail.message).into());
        }
    };
    let ordinary = JsonValue::from_serializable("item")?;
    match input.validate(&ordinary) {
        ValueOutcome::Satisfies => println!("input satisfies the contract"),
        ValueOutcome::Fails { message, problems, problems_complete } => {
            println!("{message}: {problems:?}; diagnostics complete: {problems_complete}");
        }
        ValueOutcome::NoVerdict { detail } => {
            println!("no verdict ({:?}): {}", detail.reason, detail.message);
        }
    }
    Ok(())
}
```

Retain and clone the ready contract for repeated validation. Each `prepare` call
compiles independently. The packaged [first-use example](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/first_use.rs)
adds aliases and exact wide integers; the [replacement example](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/replacement.rs)
keeps old work usable while installing a new document and resource context.

## Limits

The evaluator never fetches references. Supply resources through `ResourceSet`;
new resources require a new context. Some values can be decided despite missing
resources or undefined reference results when those results cannot affect the
verdict. A ready contract does not promise a verdict for every value. Handle
no-verdict during both preparation and validation.

`Limits` controls work, recursion, regex steps, compilation depth and pattern
admission. The default regex allowance is 2,000,000 steps. Large unanchored
searches can exhaust it and return `LimitExceeded`, including ordinary megabyte
strings. Unicode property matching is unsupported when applied. `format` is an
annotation; another declared schema dialect is not treated as 2020-12.
See [measured behavior](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md).

`Diagnostics::Coarse` is the default. An established failure has a fixed message,
an empty problem list and `problems_complete = false`. Choose
`with_diagnostics(Diagnostics::Upstream)` to request the library's first error.
That mode can allocate in proportion to inputs and overflow the stack on deep
values. Available paths map to original sources. Optional exact schema facts may
expose source operands and retain whole document/resource snapshots. Missing
or incomplete diagnostics never weaken an established failure.

Cancellation uses `WorkControl` and is cooperative. Cancelled preparation produces
no ready owner; cancelled validation leaves a ready owner usable for a later
attempt. Use a fresh control for an independent retry.

This unpublished package requires Rust 1.88. See
[support](https://github.com/openbindings/sdk/blob/main/docs/support.md) for stack
qualifications and [custom evaluators](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md)
for the integration contract.

## Reference

The crate reference documents `DefaultEvaluator`, `Limits` and `Diagnostics`.
[Resolver and evaluation maintenance](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
links the resource-identity rules, partial evaluation proof and dependency patches.
