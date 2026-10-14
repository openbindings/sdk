# Migration boundaries

The maintained reference engine is Rust, with native Rust packages and a
TypeScript API over WebAssembly in this repository. Package versions are
independent of specification versions. The specification remains authoritative.

This delivery covers document parsing and exact preservation, typed authoring,
conformance, inspection, operation value contracts, optional default schema
evaluation, evaluator test support and HTTP discovery. Optional Rust companions
also provide source inspection, interface synthesis, binding/operation invocation
through provider traits and checked caller APIs. These role APIs are provisional;
their public TypeScript counterparts and CLI/application integration are not
delivered by this SDK. These capabilities remain outside specification core.

The older Go and independent TypeScript SDKs are legacy implementations. Their
source and historical packages are preserved. The older TypeScript SDK has a
broader invocation/synthesis/adapter surface; replacing it requires checking each
caller's actual capabilities. This API is not a drop-in replacement for all of
those exports. See [CAPABILITIES.md](../CAPABILITIES.md) and the package guides for the supported scope.

The SDK supplies no kind-specific instances of its roles. The separate
[OpenAPI project](https://github.com/openbindings/openapi) implements them and
depends on the independent
[client](https://github.com/openbindings/openapi-client).
Consult that project's guide for its private kinds, supported profile and limits.

## Source provenance

The initial implementation imports qualified source commit
`dd076414006c5687c6eda8685a2569ebe55f7012` from the preserved Rust migration
candidate. The subsequent import changes repository metadata and lifecycle
instructions, and adds a reviewable repository history; the candidate's original
history and qualification artifacts remain preserved by the project.

The applied specification and Go comparison revisions are recorded in the root
README. Original qualification and independent repair review are bounded to the
capabilities they exercised. Source landing does not establish publication,
complete legacy parity, stabilization or application cutover. New CI runs and
package builds identify their own source, rather than reusing an old archive hash
as evidence of a new build.

## API quality changes in the working prerelease

Rust callers replace `resolve_operation(name)?.is_some()` or optional unwrapping
with a match on `OperationSelection`. A found view uses `key()`, `description()`,
`aliases()`, `input()`, `output()`, `bindings()` and `value()`. Malformed required
structure is a located interpretation error, independent of unrelated conformance.
Every repeated name occurrence is ambiguous; even a one-key candidate list can
represent repeated aliases in that operation.

Replace `context.prepare(name, direction).validate(value)` with a match on
`ValueContractPreparation`. Only `Ready(contract)` calls `contract.validate(value)`.
Absent schema, missing/ambiguous operation and preparation refusal are setup states.
`ValueOutcome` has exactly the three semantic evaluation branches; custom evaluator
traits return those branches and the qualification kit reports setup separately.
Every preparation compiles anew; retain and clone ready owners for reuse after document/context drop. Cancellation of
preparation yields no ready owner; retry uses a healthy context. Cancellation of a
value check leaves a retained ready contract usable.

Qualified missing-resource contracts now prepare ready under the default evaluator,
including bare missing `$ref` and known alias chains. Resource-dependent values
return `ResourceUnavailable` during validation. Existing code must handle no verdict
in both phases. Known branch-independent values can now satisfy or fail with the
original resource set; `anyOf(true, missing-U)` is a declared capability expansion.
`anyOf(true, invalid-known-pattern)` still refuses. Ready owners remain bound to
their original snapshots; supplying resources means creating a new context.
Remove calls to `resource_completeness()` / `resourceCompleteness` and the
`ResourceCompleteness` type; no replacement classification exists. Ready means
prepared, not decidable (EV-9). Applications own replacement policy and continue
to handle no verdict at both stages. Custom evaluators retain the original
`SchemaRequest` context and strict `evaluation_program()` helper. Projected entry
and resources now use read-only `entry_uri()` and `resources()` accessors. Remove
`evaluation_bounds`, `EvaluationBounds`, `original_location_bounded` and
`LocationBudgetExceeded` from adapter usage: bounds and bounded mapping are now
private evaluator details. Static reference refusals use `ReferenceRefusal` and
`ReferenceRefusalReason`, including Cancelled, separately from value no-verdict.

Ordinary `$id` and supplied-resource resolution follows library normalization and
encoded-name handling under §7.4. OBI-13's exact compared strings are unchanged.
The S6d/e migration ledger records each approved case delta. Supplied normalized
peers remain ambiguous; no spelling fallback or implicit acquisition is added.

`AuthoringError` fields are private: use `kind()`, `draft_pointer()`,
`source_location()` and `message()`. Native draft paths spell `additional_fields`;
TypeScript paths spell `additionalFields`; neither is an original source pointer.
Checked `JsonValue::from_serializable` errors use emitted JSON pointers. Oversize
conversion messages truncate at a UTF-8 boundary; oversize paths are omitted,
never shortened into a false location. Explicit accessors disclose truncation.

Checked Serde admission has an explicit 128-container ceiling; requested smaller
`JsonLimits.max_depth` values apply, and larger values do not raise the ceiling.
The existing iterative exact parse lane retains its 10,000-container default.

`JsonValue::from_serializable` accepts the documented ordinary Serde profile and
retains exact i128/u128 digits. Ordinary `serde_json::Value` numeric leaves use
the qualified pinned Number protocol and retain its token spelling. `RawValue`
and exact `JsonValue` wrappers are explicitly refused in this lane. Keep `JsonValue::parse`/views/retention for exact token carriage and pass
owned exact values to typed drafts. Converting to ordinary floating-point values
first cannot recover lost digits.

TypeScript callers match the analogous tagged selection and preparation unions,
check ordinary-value admission results, and dispose only actual found/ready owners.
Use `SchemaResources` entries for atomic batches. Import HTTP helpers from
`@openbindings/sdk/http-discovery`; it shares the root's initialization and handle
realm. Construct TypeScript validated documents through `parsed.validated()` and narrow
`status === "validated"`; that branch is the sole public proof-construction route. The package guide gives
complete before/after examples and nested exact-value ownership rules.


Optional default-evaluator schema facts use
`DefaultEvaluator::new().with_schema_details(true)` (TS
`valueContracts({ includeSchemaDetails: true })`). They retain original source snapshots
and share the diagnostic string-byte budget. `ValueProblemDetails::Truncated`
sets result completeness false; absent details mean disabled or unavailable.
Exact bounds and enum choices remain JSON token strings. Existing Rust evaluator
implementations constructing `ValueProblem` must add `details: None` to preserve
their previous output. The enum is non-exhaustive; match future variants safely.
See the [evaluator resource contract](../crates/openbindings-json-schema-evaluator/README.md).

## Validated contract access (S6c)

Move `parsed.value_contracts(evaluator, resources)?` to
`proof.value_contracts(evaluator, resources)`, where `proof` comes from
`parsed.validate()?.validated()`. TypeScript likewise moves `valueContracts()`
to the document returned by `parsed.validated()`. `operation::Invoker::new`
requires that proof, and `with_evaluator` no longer returns an interpretation error.
Retain parsed views for editing invalid documents; acquire validated views for
infallible field reads. Rust exact text now uses `JsonStringRef` for descriptions,
tags and kinds; callers choose scalar conversion or exact UTF-16 explicitly.
