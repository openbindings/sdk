# Supplemental identity authorities

These exact official JSON Schema sources support a limited identity analysis of
supplied draft-06 and v1 resources. They do not change the pinned OpenBindings
specification or interfaces, and do not establish evaluation support for either
edition. `manifest.json` records immutable revisions, source URLs and exact hashes.
`tools/verify-spec.mjs` verifies the complete seven-file inventory and tests drift.

The scanner may use the documented selectors and identifier rules to exclude a
possible collision with a custom meta-schema. It does not install those possible
identities as actual aliases or schema locations. Unknown v1 extensions, wider
identifier normalization, unsupported identifier forms, and arbitrary mandatory
consumer vocabularies leave identity certainty unresolved. Its v1 subset accepts
ASCII identifiers already in syntax-normal form; it does not pretend v1 is 2020-12.

The upstream specification authors retain their copyrights. The JSON Schema
Specification repository distributes these materials under BSD-3-Clause or AFL-3.0;
the SDK retains those license texts under
`crates/openbindings-internal-schema/schemas/`.
