# OpenBindings JSON Schema evaluator

An optional evaluator for operation input and output schemas. Core owns document
conformance, original schema identity and reference resolution; this companion
implements per-value evaluation with exact numbers and explicit external schemas.
It never fetches resources. This unpublished package requires Rust 1.88.

```rust
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let checked = validate_document(
        br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
    )?;
    let proof = checked.conformant().ok_or("document is not conformant")?;
    let schema = proof.resolve_operation("lookup")
        .ok_or("operation is missing")?.input().ok_or("input schema is absent")?;
    let prepared = schema.prepare(
        &DefaultEvaluator::new(), &ExternalSchemas::default(), &PreparationLimits::default(),
    )?;
    match prepared.evaluate_json(r#""item""#)? {
        ValueOutcome::Satisfies => println!("satisfies"),
        ValueOutcome::Fails { problems, problems_complete, .. } => {
            println!("fails: {problems:?}; diagnostics complete: {problems_complete}");
        }
        ValueOutcome::Undefined { detail } => println!("undefined: {}", detail.message),
        ValueOutcome::NoVerdict { detail } => println!("no verdict: {}", detail.message),
    }
    Ok(())
}
```

`Schema::prepare` returns `Result<PreparedSchema<DefaultValidator>, PreparationFailure>`.
A preparation failure judges no value. An absent operation or absent input/output
schema is a separate `None`. A prepared schema owns its evaluation state and can
outlive the document and resource handles. Clone it for repeated evaluation;
preparing again performs a new preparation. Supplying different external content
requires a new preparation.

`evaluate_json` parses exact text or bytes; parsing failure is an error distinct
from the four value outcomes. To reuse admitted values or choose parsing limits,
use `JsonValue::parse_with_limits` followed by `PreparedSchema::evaluate`.
Duplicate instance member names produce `NoVerdict` after parsing.

## Results and semantic dependence

`Satisfies` and `Fails` are established validity results. `Undefined` means validity
depends on a known undefined result under OpenBindings §5.5, with coherent opposing
interpretations. Unsupported features, unavailable resources, exhausted budgets
and cancellation alone do not establish undefinedness.

The evaluator first uses its compiled engine and conservative validity bounds.
When those cannot decide, a bounded analysis of original schema locations combines
predicate results with evaluated-property and evaluated-item annotations. Repeated
original predicates share the same exact JSON value identity; pattern predicates
share the same original site and tested UTF-16 text. Numeric spellings and object
member order do not split equal values. Actual occurrence identity remains separate
for recursion detection.

Malformed local reference operations and invalid Unicode patterns do not create
annotations. Known applicators produce annotations only under their corresponding
successful interpretations. Unavailable content may supply both assertions and
annotations, and may refer to identified predicates already present in the
document. The dependence proof accounts for those correlations. The anonymous OBI
document resource is unnameable from external content; its private predicates have
a different independence boundary. A conservative opaque local summary may depend
on either category.

Pure reference aliases preserve resource entry and dynamic scope before being
normalized. A pure alias cycle can establish `Undefined`. More complex recursive
components use conservative summaries; an unfinished recursive proof gives
`NoVerdict`, never an answer inferred from a traversal cut. Decisive surrounding
logic can still establish a result.

The current implementation has open capability gaps for some complex recursive
summaries. The 1,546 required migration values pass, including standard external
dialects and custom vocabulary/format cases. Finite passing suites do not
establish complete JSON Schema support.

## Limits and cancellation

```rust,edition2024
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator, Limits};

fn prepare_bounded(schema: &Schema, resources: &ExternalSchemas)
    -> Result<PreparedSchema<DefaultValidator>, PreparationFailure>
{
    let evaluator = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: Some(10_000_000),
        schema_resources: Some(1_000),
        ..Limits::default()
    });
    schema.prepare(&evaluator, resources, &PreparationLimits::default())
}
```

| Limit | Default | What it bounds |
| --- | --- | --- |
| `evaluation_steps` | no cap | Evaluation and semantic proof work |
| `schema_resources` | no cap | Reached original resource construction |
| `stack_bytes` | 256 KiB | Compilation, evaluation and proof nesting |
| `compile_json_depth` | 512 | Projected JSON sent to the compiled engine |
| `pattern_bytes` | 1 MiB | One pattern source and its expanded form |
| `pattern_depth` | 32 | Pattern group nesting |
| `total_pattern_bytes` | 4 MiB | Retained pattern forms |
| `total_pattern_memory` | 64 MiB | Matchers and their engine allowances |
| `program_bytes` | 48 MiB | SDK program growth and semantic proof storage |

`PreparationLimits` bounds the original schema environment's footprint and the
whole preparation attempt's work and conservative allocation reservations. A local
undefined operation cannot bypass construction accounting: the semantic path
walks known original references and applicators, counts resources and checks
pattern budgets before becoming ready. Proof work, exact effect keys, annotations
and decision-graph storage are charged, including custom meta-schema admission
and retries after discovering a recursive component. Supported pattern matching borrows the input string;
unknown matches reserve their exact text-key allowance before constructing it.

These limits are not a process RSS guarantee and do not include caller-owned
inputs or all memory held by the underlying compiled library. Algorithm and
allocation checks live in the evaluator tests and `tools/diagnostic-memory-probe`.
Final matched-toolchain performance qualification remains part of the migration.

Cancellation is cooperative through `WorkControl`. Cancelled preparation produces
no prepared owner; a cancelled evaluation does not poison cached predicates or
prevent a later uncancelled retry. A smaller budget can replace an answer with an
honest inability, but cannot manufacture `Undefined` or reverse validity.

## Patterns, exact values and diagnostics

Patterns follow ECMA-262 11th edition with Unicode semantics. The linear matcher
supports the translated subset, including the qualified Unicode property tables
and ASCII word boundary. Unsupported lookaround/backreferences, missing property
tables and non-word-boundary assertions retain explicit capability refusals when
validity depends on them. An invalid expression instead supplies a known undefined
match. `patternProperties`, `additionalProperties` and unevaluated keywords use
that same match choice coherently.

Numbers are compared exactly, including large integers and exponents. Expensive
numeric arithmetic can hit its explicit limit. JSON strings retain unpaired
UTF-16 units; exact equality preserves them, while unsupported Unicode-text
operations report their limitation. `format` is an annotation where its assertion
is optional. Required format assertion supports all 19 standard attributes;
unknown required attributes refuse preparation. Its `regex` attribute checks
ECMA-262 11th-edition no-flags syntax with Annex B, without executing a pattern.
An external resource's declared dialect is never silently read as 2020-12.
See [evaluator profiles and preparation](../../docs/schema-evaluators.md) for
custom meta-schema admission and the remaining backend allocation gate.

`evaluate` is the cheap check: failure has no problems and marks diagnostics
incomplete. `explain` preserves the same result and reports available bounded
problems at original source locations. Some semantic failures currently have no
specific diagnostic witness and retain `problems_complete = false`. Optional
detail work never retracts an established result. Oversized origins are omitted
before copying them, pattern diagnostics do not echo authored regex text, and an
unavailable resource has no invented source coordinate.

Complete compiled validators retain their compiled state and original locations.
Validators needing semantic fallback additionally retain the immutable original
schema environment and successful primitive preparations. No caller borrow is
retained. See [custom evaluators](../../docs/schema-evaluators.md),
[first use](examples/first_use.rs), and [replacement](examples/replacement.rs).
