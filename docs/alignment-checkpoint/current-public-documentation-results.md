# Current public terminology, API guidance and ownership

The public guides, Rustdoc/JSDoc and runnable TypeScript examples now use schemas,
PreparedSchema, ExternalSchemas and evaluate/evaluateJson. Document validation
remains a separate conformance operation. Immutable edit semantics, owned original
schema context and current optional Rust role boundaries replace obsolete mutable
setters, source-release claims and old value-check guidance. Historical performance
receipts retain their original measurements and are explicitly revision-bound.

The default evaluator's advisory messages now name schemas. Existing diagnostic
redaction and arithmetic tests retain their assertions with the current wording;
no validity, limit default, capability oracle or compatibility shim was changed.
The dedicated removed-diagnostics option guard is deleted. Generic strict unknown-
option rejection remains, covered at runtime and in TypeScript negative controls.
The zero-source-arena assertions for particular compiled fixtures are explicitly
fixture-specific and do not constrain the original-engine architecture.

[Independent review](current-public-documentation-review.md) corrected the distinction between missing ordinary evaluation
references and missing required meta-schema admission, scoped historical stack
measurements, and clarified pattern-form deduplication. No findings remain in this
bounded slice. It is not the whole specification/code-map completion claim.

Validation: 283 Rust evaluator/test-support checks and doctests pass (three existing
release-only controls ignored); Rust public-reference/doctests and verbatim guide
snippets pass. Actual rebuilt Wasm passes 180 Node tests, TypeScript type/reference
checks and the installed npm archive (352 assertions plus runnable README and
lifecycle/editing examples). Chromium and WebKit each pass 505 core cases and
1,546 evaluator cases, including the updated delivered editor pages. Product-label,
documentation-link and formatting checks pass. Final normalized-pattern wording is
a comment-only correction after those semantic host runs; the bridge/declarations
are rebuilt with the final source fingerprint and their API references rechecked.
The final Wasm and JavaScript artifact hashes are identical to the semantic-host
run, confirming that the final correction changed documentation only.

The unchanged pinned inputs remain spec c1419c9 and interfaces 5815fb1. The next
original-engine increment follows the separately reviewed bounded numeric-reading
design; eager-all preparation and removal of comparison scratch accounting are
explicitly rejected. No default cap increase or compatibility preservation is
required or proposed.
