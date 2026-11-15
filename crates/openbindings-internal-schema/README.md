# OpenBindings internal schema

Implements shared fixed document/schema admission and the original-schema resource
graph. It separates the normative contained OBI walk from external interpretation
profiles, while retaining original identities, references and source locations.
Evaluation and comparison share this crate's `canonical_resolved_uri` normalizer
and `normalized_hierarchical_uri_text` shortcut for JSON Schema resource equivalence.
The `uri`
module keeps spelling-preserving resolution for document conformance and
retrieval registration, whose identity rules are separate.
Applications use `openbindings` for document validation and an explicit schema
evaluator for operation values.

This package is a delivery dependency, not a supported extension API. It is
unpublished and requires Rust 1.88. Start with the
[SDK guide](https://github.com/openbindings/sdk) for a working application example.
Implementation and dependency contracts are in the
[maintainer guide](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md).
