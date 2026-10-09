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
