# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- Focused Rust and TypeScript first-use and retained-replacement guides with
  runnable packaged examples and explicit host initialization.
- Exact Rust document model, typed authoring, all 13 document rules, indexed
  operation names/aliases, opaque kind checks and original-resource references.
- Explicit resources, retained input/output contracts, optional default 2020-12
  evaluator and reusable evaluator qualification kit.
- Optional HTTP discovery client and immutable publication helper.
- First-class TypeScript/Wasm facade for browsers, Node ESM and local workerd.
- Locked development builds, public consumers, source identity checks, portable
  corpus replay, dependency attribution and Linux/macOS/Windows CI definitions.

### Changed

- Fixed-schema diagnostics explain direct type and missing-required-field failures
  while preserving their codes, original locations and conformance results; other
  cases retain a generic explanation without exposing rejected values.
- Documented the TypeScript storage counter as shared exact-JSON arenas, including
  lazy fixed evaluator storage and the distinction from handles or memory bytes.
- Operation lookup now returns explicit found/missing/ambiguous selection; retained
  operation views have typed accessors and located structural refusals.
- Contract preparation now returns ready/no-contract/operation-missing/
  operation-ambiguous/no-verdict. `ValueOutcome` contains only satisfies, mismatch
  and no-verdict. The evaluator kit separates preparation and evaluation evidence.
- Native authoring errors expose stable kinds and logical draft paths; checked
  Serde conversion admits ordinary values with explicit profile/limits and owned,
  bounded diagnostics. Exact parse/retain remains available.
- TypeScript validation accepts ordinary values through `ValueCheck`, resources
  accept atomic batches, interpretation errors carry locations, and metadata is
  readonly. HTTP discovery exports move to `@openbindings/sdk/http-discovery`,
  sharing one initialization/handle realm. Public `ValidatedDocument.fromParsed`
  is removed; conformance assessment provides proof construction.

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
