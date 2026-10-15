# OpenBindings JSON Schema evaluator

Validate values against an operation's input or output value contract. This
optional companion supports JSON Schema 2020-12, exact JSON numbers and explicit
immutable schema resources. Use it alongside `openbindings`.

```rust
use openbindings::{Direction, ParsedDocument, ValueOutcome};
use openbindings_json_schema_evaluator::value_contracts;

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
    let context = value_contracts(&proof);
    let input = context.prepare("lookup", Direction::Input).into_result()?;
    match input.validate_json(r#""item""#)? {
        ValueOutcome::Satisfies => println!("input satisfies the contract"),
        ValueOutcome::Fails { message, problems, problems_complete } => {
            println!("{message}: {problems:?}; diagnostics complete: {problems_complete}");
        }
        ValueOutcome::Undefined { detail } => {
            println!("undefined result: {}", detail.message);
        }
        ValueOutcome::NoVerdict { detail } => {
            println!("no verdict ({:?}): {}", detail.reason, detail.message);
        }
    }
    Ok(())
}
```

`value_contracts(&proof)` uses `Limits::default()`, coarse diagnostics and no extra
resources. `into_result()` preserves every non-ready reason in
`ValueContractPreparationError`, including the original no-verdict detail as its
error source. Match `ValueContractPreparation` directly when setup states need
individual handling. `validate_json` admits exact text or bytes under the default
JSON limits; `Err` means admission failed and `Ok` contains any of the four value
outcomes. Duplicate members yield no-verdict after admission.

Use `ValidatedDocument::value_contracts(evaluator, resources)` to share or
configure an evaluator or supply resources. For example:

```rust,edition2024
use openbindings::{ResourceSet, ValidatedDocument, ValueContracts};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Diagnostics};
use std::sync::Arc;

fn with_resources(proof: &ValidatedDocument, resources: ResourceSet) -> ValueContracts {
    let evaluator = Arc::new(DefaultEvaluator::new().with_diagnostics(Diagnostics::Upstream));
    proof.value_contracts(evaluator, resources)
}
```

Use `JsonValue::parse_with_limits` for custom admission limits, or
`JsonValue::from_serializable` for ordinary Rust values. Keep admitted exact values
and call `ValueContract::validate` to check them repeatedly without reparsing.

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

`ValueOutcome::Undefined` establishes that whether the value satisfies its value
contract is undefined. The default evaluator proves this for static reference
defects: pointers that reach no schema, missing or duplicate plain names, and
malformed `$ref` values within resources declaring `$id`. It requires original
provenance and two admissible completions that disagree. Unsupported fragments,
missing resources and inconclusive analysis remain no-verdict. `UndefinedResult`
retains a code, message and optional original schema location.

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

The crate reference documents `value_contracts`, `DefaultEvaluator`, `Limits` and `Diagnostics`.
[Resolver and evaluation maintenance](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
links the resource-identity rules, partial evaluation proof and dependency patches.
