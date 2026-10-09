# Migration boundaries

The maintained reference engine is Rust, with native Rust packages and a
TypeScript API over WebAssembly in this repository. Package versions are
independent of specification versions. The specification remains authoritative.

This delivery covers document parsing and exact preservation, typed authoring,
conformance, inspection, operation value contracts, optional default schema
evaluation, evaluator test support and HTTP discovery. It does not yet provide
operation invocation, source synthesis, binding adapters or CLI/application
migration. Those capabilities remain optional layers outside specification core.

The older Go and independent TypeScript SDKs are legacy implementations. Their
source and historical packages are preserved. The older TypeScript SDK has a
broader invocation/synthesis/adapter surface; replacing it requires checking each
caller's actual capabilities. This API is not a drop-in replacement for all of
those exports. See CAPABILITIES.md and the package guides for the supported scope.

The independent dynamic OpenAPI client lives in
[openbindings/openapi-client](https://github.com/openbindings/openapi-client).
An OpenBindings adapter may compose that engine with SDK facilities; the engine
has no dependency on OpenBindings core.

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

Replace `context.prepare(name, side).validate(value)` with a match on
`ContractPreparation`. Only `Ready(contract)` calls `contract.validate(value)`.
Absent schema, missing/ambiguous operation and preparation refusal are setup states.
`ValueOutcome` has exactly the three semantic evaluation branches; custom evaluator
traits return those branches and the qualification kit reports setup separately.
Ready owners survive document/context drop and cache eviction. Cancellation of
preparation yields no ready owner; retry uses a healthy context. Cancellation of a
value check leaves a retained ready contract usable.

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
realm. Construct validated documents through assessment. The package guide gives
complete before/after examples and nested exact-value ownership rules.
