# OpenBindings JSON Schema evaluator

Validate values against an operation's input or output value contract. This
optional companion supports JSON Schema 2020-12, exact JSON numbers and explicit
immutable schema resources. Use it alongside `openbindings`.

```rust
use openbindings::{Document, ValueOutcome};
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = Document::parse(
        br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
    )?;
    let validation = document.validate()?;
    if validation.conformant().is_none() {
        return Err(format!("document conformance: {:?}; findings: {:?}",
            validation.report().conclusion, validation.report().findings).into());
    }
    let proof = validation.conformant().ok_or("document is not conformant")?;
    let operation = proof.resolve_operation("lookup").ok_or("operation is missing")?;
    let input = operation
        .input()
        .ok_or("operation has no input contract")?
        .prepare(&DefaultEvaluator::new())?;
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

`prepare(&DefaultEvaluator::new())` uses `Limits::default()` and no supplied
resources. A missing operation and an absent contract are `None`;
preparation refusal is a `NoVerdict` error that judges no instance. Ready means
prepared, not decidable. `validate_json` admits exact text or bytes under the
default JSON limits; `Err` means admission failed and `Ok` contains any of the
four value outcomes. Duplicate members yield no-verdict after admission.

Configure the evaluator or supply resources with `prepare_with`. The evaluator is
borrowed for the call, and one `ResourceSet` can serve many preparations:

```rust,edition2024
use openbindings::{NoVerdict, PreparedContract, ResourceSet, ValueContract};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn with_resources(
    contract: ValueContract<'_>,
    resources: &ResourceSet,
) -> Result<PreparedContract, NoVerdict> {
    let evaluator = DefaultEvaluator::with_limits(Limits {
        regex_steps: 100_000,
        ..Limits::default()
    });
    contract.prepare_with(&evaluator, resources)
}
```

Use `JsonValue::parse_with_limits` for custom admission limits, or
`JsonValue::from_serializable` for ordinary Rust values. Keep admitted exact values
and call `PreparedContract::validate` to check them repeatedly without reparsing.

Retain and clone the prepared contract for repeated validation. Each `prepare`
call compiles independently. The packaged [first-use example](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/first_use.rs)
adds aliases and exact wide integers; the [replacement example](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/replacement.rs)
keeps old work usable while installing a new document and resource set.

## Limits

The evaluator never fetches references. Supply resources through `ResourceSet`;
new resources require a new preparation. Some values can be decided despite missing
resources or undefined reference results when those results cannot affect the
verdict. A prepared contract does not promise a verdict for every value. Handle
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

Each call chooses its detail. `validate` is the cheap check: an established
failure has a fixed message, an empty problem list and `problems_complete =
false`. `explain` returns the library's first error for the same verdict, with
redacted messages and original schema facts. Building that error can allocate in
proportion to the schema and value and can overflow the stack on deep values, so
keep `validate` on paths that check untrusted input. Each explained problem
names the failing node by its exact pointer from the value passed (`instance`)
and the failing keyword by resource and exact pointer (`schema`); a keyword the
original sources do not hold is named by the selected schema itself, never
guessed. `JsonValue::locate`, `Conformant::locate` and `ResourceSet::locate`
find line and column on demand, so an explanation costs no position. Schema facts may expose
source operands. A prepared contract keeps the text of the schemas it reached
for these facts, never the document, its supplied resources or any position.
Missing or incomplete diagnostics never weaken an established failure.

Cancellation uses `WorkControl` and is cooperative. Cancelled preparation produces
no prepared owner; cancelled validation leaves a prepared owner usable for a later
attempt. Use a fresh control for an independent retry.

This unpublished package requires Rust 1.88. See
[support](https://github.com/openbindings/sdk/blob/main/docs/support.md) for stack
qualifications and [custom evaluators](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md)
for the integration contract.

## Reference

The crate reference documents `DefaultEvaluator` and `Limits`.
[Resolver and evaluation maintenance](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
links the resource-identity rules, partial evaluation proof and dependency patches.
