# OpenBindings JSON Schema evaluator

This optional companion evaluates OpenBindings value contracts using an explicit immutable resource context. It supports JSON Schema 2020-12 and exact JSON numbers. It never acquires HTTP or filesystem resources.

Use both `openbindings = "=0.2.0-alpha.1"` and this companion in your Cargo
manifest. Packages are currently unpublished: consume locally built archives with
patches for their unpublished siblings, or use an exact source checkout. Rust needs
no runtime initialization.

```rust
use openbindings::{ValueContractPreparation, JsonValue, ParsedDocument, ResourceSet, Direction, ValueOutcome};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = ParsedDocument::parse(
        br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
    )?;
    let assessment = document.validate()?;
    if assessment.validated().is_none() {
        return Err(format!("document conformance: {:?}; findings: {:?}",
            assessment.report().conclusion, assessment.report().findings).into());
    }
    let proof = assessment.validated().ok_or("document is not conformant")?;
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


A prepared contract retains its resources after the document/context is dropped. Every `prepare` call compiles independently, including concurrent calls and aliases of the same operation. Retain and clone the returned `ValueContract` for repeated validation without recompilation; dropping its final owner releases compiled state. Failed or cancelled preparation retains nothing in the context, and a fresh call can retry. Different contexts never share resource identities or compiled contracts.

`NoVerdict` distinguishes unsupported capabilities, conservative preparation,
missing resources, limits and cancellation. After complete-resource preparation
finds a missing static-reference carrier, the evaluator can prepare positive
lower/upper validity bounds. Missing optional content may be irrelevant to an
instance; an independent known failure or passing disjunct can also decide it.
Values that depend on missing content return `ResourceUnavailable` at validation.
A qualified bare missing `$ref` now prepares `Ready` and refuses every admitted
value at validation. Ready promises a validator, not a decidable instance.

Hole influence is allowed through static `$ref`, `allOf`, `anyOf`, object property
applicators, `dependentSchemas`, `prefixItems` and `items`. Dependent `oneOf` uses
memoized lower/upper variants and can decide zero possible, one certain sole, or
multiple certain winners. It does not infer correlations between unknown branches.
Hole-dependent `not`,
conditionals and `contains` refuse; closed subgraphs using them remain
intact. Evaluated `unevaluated*` and dynamic keywords reject partial bounds.
Known malformed schemas, references, dialects and potential in-place cycles keep
refusing. No I/O occurs. `ParsedDocument::references()` exposes document-wide
reference spellings and locations; new supplied resources require a new context.
Existing prepared owners retain their original snapshots.

Within that immutable context, a unique contained declared ID owns its name ahead
of supplied resources and packaged standards. Supplied retrieval aliases and
canonical IDs otherwise compete strictly. Reaching a competing canonical carrier
through an alias, pointer or direct applicator returns `ConservativePreparation`
with `resource-identity-conflict`; it is not a missing-resource hole or value failure.
Unused conflicting resources remain isolated, while independently identified nested
resources retain their own original base and dialect. A selected resource's missing
fragment never falls through to another carrier. Original supplied rows and location
provenance remain intact: a diagnostic source URI can differ from the selected
lookup association for that URI. New identity-refusal locations are optional and
measure source URI plus full pointer against 64 KiB before copying. Ordinary resolution follows the library's documented APIs and URI normalization.
OBI-13 alone retains exact compared spelling after RFC §5.2 resolution and
empty-fragment removal; §7.4 delegates ordinary resource identity to JSON Schema. Fixed OBI and
meta-schema assessment uses its separate trusted loader.

Both bound programs are closed, but neither alone is equivalent to the partial
schema. Bounds planning and paired compilation are private evaluator modules.
BYO evaluators receive the original context and strict `evaluation_program()`;
core never retries their refusals. Upper/lower
share one verdict work/regex allowance. Only established upper failure produces
original-source diagnostics when Upstream is selected. An upper oneOf first error
produces a cardinality-neutral original `/oneOf` summary.
Only the outer `validate()` first error is used; generated contexts are not flattened,
and unmapped errors are omitted with incomplete diagnostics. The evaluator preserves
these boundaries; a structured backend output API can omit the unary summary event.
Dependent-oneOf plans additionally cap both copies at 100,000 emitted schema nodes
and 200,000 reference/applicator edges. Definition sharing is not runtime caching,
and linear syntax makes no speed/compiled-memory promise. Partial plans
admit 64 MiB combined retained text and explicit graph/scratch limits. See the
[private proof and admission contract](../../docs/partial-resource-bounds.md).
There is no annotation-output or arbitrary-future-resource-readiness guarantee.

Unicode property-escape *syntax* follows ECMA-262 edition 11; actual property
matching is explicitly unsupported and declined only when applied. `format`
remains an annotation. Custom/historical dialects are not interpreted as 2020-12.

Regex compilation refusals use `schema-pattern-compilation` with bounded guidance
to inspect `pattern` and `patternProperties`. They do not echo patterns or resource
identifiers. A source location remains absent when the compiler cannot establish
the original resource; other unclassified preparation failures retain the generic
`evaluator-preparation` diagnostic.

The default evaluator decides validity with `is_valid`. `Diagnostics::Coarse`
(the default) returns `Fails` with the fixed message “value does not satisfy the
contract”, no problems, and `problems_complete = false`. TypeScript uses
`diagnostics: "coarse"`, `message`, and `problemsComplete: false`.

Select `DefaultEvaluator::new().with_diagnostics(Diagnostics::Upstream)` or
`proof.valueContracts({ diagnostics: "upstream" })` to request the library's own
first error through `validate()`. Messages remain value-free; available paths map
to original locations and optional schema facts may expose source operands.
This mode is **library-bounded**: construction can allocate in proportion to inputs
and can overflow the stack on deep values. The SDK promises nothing stronger than
the library. One first error does not establish diagnostic completeness, so the
flag remains false. Work/regex limits and cancellation can omit detail after a
verdict; missing detail never weakens failure and never counts as rule evidence.

The former `with_schema_details` / `includeSchemaDetails` boolean and the
`max_problems` / `maxProblems`, `diagnostic_bytes` / `diagnosticBytes` options are
removed. There is no intermediate boolean diagnostics API or SDK construction
bound for upstream errors.

`Limits` bounds evaluation work, recursion, regex work, dependency compilation depth and pattern admission. Parsing, graph preparation and dependency compilation use separate admission bounds. Cancellation is cooperative; dependency compilation/evaluation has bounded regions that are not preempted midway. Browser applications should use a Worker for large synchronous jobs.

The vendored adaptation is an implementation detail. Upgrade it with the recorded patch invariants and the complete evaluator contract suite; a dependency draft label alone is insufficient evidence.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Package publication and application cutover are separate actions.

`WorkControl` is a synchronous trait accepted as `&dyn WorkControl`. A plain
`AtomicBool` or an async companion’s `CancellationToken` can supply it. Cancellation
is permanent within an attempt; use a fresh flag or token for a healthy retry. Cancelled preparation returns no ready owner, while cancelled validation
leaves an existing contract usable. The application decides whether a candidate is
ready to replace active work and controls its scheduling. Releasing the final Rust
owner is distinct from reducing allocator RSS.

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](../../docs/api-reference.md).


### Optional exact schema facts

Upstream mode retains original document/resource snapshots for `ValueProblem.details`.
These can extend the lifetime of whole source arenas, including unrelated members.
Replacement resources do not change facts held by an existing contract. Coarse mode
adds no original snapshot owners for diagnostics.

The non-exhaustive `ValueProblemDetails` enum carries type names, a verified missing
required member, exact numeric/size bounds, or enum choices as JSON token strings.
Interpret a bound with the problem's keyword code; never convert exact tokens through
`f64`. Unsupported or interrupted fact recovery leaves detail absent. The `Truncated`
variant remains available to custom evaluators; the default no longer has a byte
allowance or emits a truncation marker.
