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

`NoVerdict` distinguishes unsupported capabilities, conservative preparation,
missing resources, limits and cancellation. After complete-resource preparation
finds a missing static-reference carrier, the evaluator can prepare positive
lower/upper validity bounds. Missing optional content may be irrelevant to an
instance; an independent known failure or passing disjunct can also decide it.
Values that depend on missing content return `ResourceUnavailable` at validation.
A qualified bare missing `$ref` now prepares `Ready` and refuses every admitted
value at validation. Ready promises a validator, not a decidable instance.

Hole influence is allowed through static `$ref`, `allOf`, `anyOf`, object property
applicators, `dependentSchemas`, `prefixItems` and `items`. Hole-dependent `not`,
`oneOf`, conditionals and `contains` refuse; closed subgraphs using them remain
intact. Evaluated `unevaluated*` and dynamic keywords reject partial bounds.
Known malformed schemas, references, dialects and potential in-place cycles keep
refusing. No I/O occurs. `ParsedDocument::references()` exposes document-wide
reference spellings and locations; new supplied resources require a new context.
Existing prepared owners retain their original snapshots.

Both bound programs are closed, but neither alone is equivalent to the partial
schema. Rust adapter authors may opt into `SchemaRequest::evaluation_bounds` and
consume `(lower, upper, unavailable)` with `EvaluationBounds::into_parts`; custom
evaluator dispatch and strict `evaluation_program()` are unchanged. Upper/lower
share one verdict work/regex allowance. Only established upper failure produces
known-source diagnostics, in the existing separate diagnostic scope. Partial plans
admit 64 MiB combined retained text and explicit graph/scratch limits. See the
[complete proof, API and admission contract](../../docs/partial-resource-bounds.md).
There is no annotation-output or arbitrary-future-resource-readiness guarantee.

Unicode property-escape *syntax* follows ECMA-262 edition 11; actual property
matching is explicitly unsupported and declined only when applied. `format`
remains an annotation. Custom/historical dialects are not interpreted as 2020-12.

Regex compilation refusals use `schema-pattern-compilation` with bounded guidance
to inspect `pattern` and `patternProperties`. They do not echo patterns or resource
identifiers. A source location remains absent when the compiler cannot establish
the original resource; other unclassified preparation failures retain the generic
`evaluator-preparation` diagnostic.

Failures report actual instance locations and original schema locations where available. They do not promise every possible failing keyword or Go's diagnostic multiplicity. `problems_complete` means the selected diagnostic pass completed without truncation, not exhaustive traversal of every semantically redundant failure. Diagnostic collection has its own work scope and allocation cap; a confirmed failure remains a failure when its diagnostics are truncated. Messages omit instance values. For partial schemas, completeness describes the selected upper diagnostic pass and makes no claim about unavailable content.


Value failure diagnostics use `Limits::diagnostic_bytes` (TypeScript
`diagnosticBytes`), default **1,048,576 UTF-8 bytes per result**. The total includes
every retained instance pointer, schema resource URI and pointer, code, message and every requested detail string (including its kind tag).
The evaluator retains a deterministic prefix of whole problems; it never clips a
pointer or invents a location. `max_problems` / `maxProblems` still defaults to 256
and has a minimum count allowance of one. A byte allowance need not fit one
problem: zero, or a single oversized location, can produce `fails` with an empty
list and `problems_complete = false` / `problemsComplete: false`. Established
success and failure do not change when this allowance is lowered. Cancellation
and pre-verdict work exhaustion retain their existing no-verdict semantics.

The byte allowance is neither a heap cap nor a wire cap. For the current default
problem shape, compact UTF-8 JSON is conservatively bounded by `59 + 96*N + 6*B`
bytes for Rust/Wasm transport, and `58 + 94*N + 6*B` for `JSON.stringify` of the
built-in TypeScript failure result. `N` is the retained problem count and `B` the
retained UTF-8 string-byte total. The envelope, field names, null resource and
comma framing are included; JSON escaping costs at most six bytes per UTF-8 byte.
Pretty printing, application wrappers and custom evaluators are outside this bound.

Before collecting final problems, the private validator separately admits at most
`diagnostic_bytes` logical bytes of copied instance paths and member-name strings,
and at most `8 * max(max_problems, 1)` error records and collection entries each.
Unused nested applicator error trees, rejected item values and pattern/schema
payloads are omitted before copying. Required names are omitted by default;
opt-in required names share this scratch allowance, with optional refusal recorded
separately so an already established base failure can remain. The final adapter walks
collections one member at a time, sizes JSON Pointer escaping before allocation,
and admits original resource/pointer copies before constructing each problem.
Original generated-URI decoding has its own same-sized scratch allowance; normal
unescaped generated identifiers are borrowed. Fixed keyword/type message text is
at most 192 bytes. Counters measure admitted logical data, not allocator calls or
capacities. Arc/buffer copies, vector headers/capacity, evaluation bookkeeping,
source admission, schema compilation, retained source maps and facade transport
copies remain distinct costs. Exhausting a private diagnostic allowance may produce
a shorter prefix even when some final byte allowance remains. These guarantees
apply to the default evaluator and built-in facade, not arbitrary application
implementations of `SchemaEvaluator`.

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


### Optional exact schema facts

`DefaultEvaluator::new().with_schema_details(true)` enables `ValueProblem.details`.
It defaults to false and is separate from `Limits`: disclosure is an application
choice, not a resource limit. Default messages explain keyword semantics without
copying schema operands or rejected values. Custom evaluators choose their own
policy.

The non-exhaustive `ValueProblemDetails` enum carries expected type names, a
verified missing required member, an exact numeric/size bound, or complete enum
choices as original JSON token strings. Match `code` to distinguish inclusive,
exclusive, lower and upper bounds. Numeric strings never pass through `f64`.
Required problems keep the existing object's instance pointer. Unsupported facts
have no detail; `None` means disabled or unavailable. `Truncated` exclusively means
requested applicable facts did not fit the budget. It makes `problems_complete`
false, including when all base problems are present.

Opt-in compiled contracts retain their original document and supplied resource
snapshots for exact source recovery. This can extend the lifetime of whole source
arenas, including unrelated document members, until the contracts/context release
them. Resource replacement creates new snapshots; existing contracts keep their
old facts. The default evaluator does not add these original snapshot owners.

Each opt-in problem reserves nine string bytes for `Truncated` before admission.
If even the base and marker cannot fit, the whole problem is omitted. Facts are
preflighted against borrowed original nodes before copying. Enum tokens and arrays
are atomic: no clipped token or partial choices list is returned. The same work and
cancellation limits apply to source lookup and detail preflight. These operations
can leave an incomplete prefix when interrupted.

With details, compact Rust/Wasm JSON and TypeScript `JSON.stringify` failure output
are both conservatively bounded by `59 + 160*N + 9*B` UTF-8 bytes. `B` includes all
retained strings and kind tags; JSON escaping costs at most `6*B`. Every enum token
is at least one byte, so at most `B` entries add at most `3*B` bytes of quotes and
commas. The fixed envelope and base/detail field framing fit the remaining terms;
type lists use a fixed seven-name vocabulary. This bounds serialization, not total
heap, vector headers/capacity, the retained source snapshots, or transport copies.

Prepared owners declare resource completeness through
`PreparedContract::resource_completeness()`: Complete for strict preparation,
Incomplete with borrowed missing-reference evidence for qualified partial preparation.
Complete does not promise total decidability. Services may require Complete before
replacement; partial previews may accept Incomplete. The evidence is one contract
witness, not necessarily the reference activated by a particular value. Optional
planner qualification/admission declines restore the original strict located
ResourceUnavailable detail in this default evaluator. Explicit `evaluation_bounds()`
callers receive the original planner cause. See the [recovery policy](../../docs/partial-resource-bounds.md).
