# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- Definition-level Rust and TypeScript API reference contracts, compiled reference
  examples, and required checks for missing documentation and broken Rust links.
- Consumer feature-matrix and source-identity guards for the private exact-number
  dependency graph.

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

- Isolate the exact-number engine's Serde JSON features under private package
  identities so ordinary consumer `serde_json` decimals keep their own feature
  behavior. Shared application dependencies now allow compatible updates; tightly
  coupled internal packages remain exact. Two additional private packages contain
  byte-identical upstream Rust sources, with maintained provenance checks.
- Bound diagnostic location materialization before collection, batch original-source
  coordinate lookup, and cap all retained conformance pointers at 8 MiB total.
  Truncation never suppresses the computation of normative rule evidence.

- Rename the established value-failure outcome to Rust `ValueOutcome::Fails` and
  TypeScript `outcome: "fails"`, including serialized results. The old `Mismatch` /
  `"mismatch"` spellings are not aliases. This is a pre-stabilization API change.
- Contract contexts and operation resolution now validate the complete operation
  namespace before selection. Malformed operations or aliases, including unrelated
  entries, return the specific `InterpretationError`; draft metadata evaluation
  remains available without requiring whole-document conformance.
- `NoVerdictReason` and `InterpretationError` are non-exhaustive cause families.
  Semantic outcome partitions remain closed, and diagnostic structs remain
  publicly constructible by custom evaluator implementations.
- TypeScript `SdkError.interpretationCode` preserves specific interpretation
  failures alongside the broad `code`, message and original source location.
- Default reference/resource and evaluator preparation messages omit source
  identifiers. Value type messages list only fixed JSON type names. Runtime cycle
  guards report `evaluation-cycle` with a conservative reason, never a work-limit
  code or an unsupported claim of semantic undefinedness.

- Unexpected normative document fields now identify their original key tokens and
  explain the `x-` extension convention. One aggregate finding can become several;
  source order within each object is preserved. Expansion shares the 4,096-finding
  cap and has an 8 MiB pointer budget. Omitted findings set `findings_truncated`
  (`findingsTruncated` in TypeScript); all rule evidence is still computed. More
  precise findings can displace later retained findings under these bounds.
- Name diagnostics explain the existing ASCII grammar without changing that rule.
  Diagnostic text is advisory.

- Made first-use Rust Markdown examples directly runnable, added routine example
  and installed README checks, and demonstrated same-owner cancellation recovery.
- Browser first-use examples display module-loading errors; browser checks cover
  missing modules and report loading failures directly.
- Fixed-schema diagnostics explain direct type and missing-required-field failures
  while preserving their codes, original locations and conformance results; other
  cases retain a generic explanation without exposing rejected values.
- Documented the TypeScript storage counter as shared exact-JSON arenas, including
  lazy fixed evaluator storage and the distinction from handles or memory bytes.
- Operation lookup now returns explicit found/missing/ambiguous selection; retained
  operation views have typed accessors and located structural refusals.
- Contract preparation now returns ready/no-contract/operation-missing/
  operation-ambiguous/no-verdict. `ValueOutcome` contains only satisfies, fails
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
