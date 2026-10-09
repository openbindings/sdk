# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- Exact Rust document model, typed authoring, all 13 document rules, indexed
  operation names/aliases, opaque kind checks and original-resource references.
- Explicit resources, retained input/output contracts, optional default 2020-12
  evaluator and reusable evaluator qualification kit.
- Optional HTTP discovery client and immutable publication helper.
- First-class TypeScript/Wasm facade for browsers, Node ESM and local workerd.
- Locked development builds, public consumers, source identity checks, portable
  corpus replay, dependency attribution and Linux/macOS/Windows CI definitions.

### Changed

- TypeScript authoring now returns structured expected-data failures with stable
  codes and escaped caller-draft pointers. These failures previously could throw
  generic TypeErrors; the error payload now describes drafts rather than byte input.
  Initialization/disposed handles and unexpected failures retain thrown semantics.
- TypeScript `ValidatedDocument` now has a private instance brand. Parsed handles
  require validate-and-narrow before publication or assignment; runtime checks remain.
- Clarified the 17 upstream disposition witness mappings and historical benchmark
  setup, cache, value-parsing and isolated-stage boundaries without rewriting old evidence.

- Preparation refusal no longer overclaims semantic undefinedness. Diagnostic
  multiplicity and native language API shapes intentionally differ from Go.
- Exact carriage and comparison remain available beyond bounded expensive
  arithmetic; implicit preparation retention uses a configurable four-entry cache.

This working draft is not published and does not retire existing Go/TypeScript SDKs.
