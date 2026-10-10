# Changelog

## 0.2.0-alpha.1 (working draft)

- Opt-in typed exact schema details through Rust `with_schema_details(true)` and
  TypeScript `includeSchemaDetails: true`, with atomic bounded truncation and
  explicit original-source retention. Default messages explain built-in keywords
  without source operands. Rust `ValueProblem` literals now require `details: None`
  (or a chosen typed detail); this is an intentional source compatibility change.
- The delivered editor uses an owned module Worker, selected exact input,
  explicit detail disclosure, stale-result suppression and UTF-8/UTF-16 caret mapping.
- Close a metadata property-name copy gap during nested applicator validity rechecks;
  scratch refusal now occurs before materialization and leaves diagnostics incomplete.


### Added

- Rust adapter utility `SchemaRequest::evaluation_bounds` and opaque owned
  `EvaluationBounds`, with borrowed lower/upper/evidence access and consuming
  `into_parts()`. The two closed programs are validity bounds, not individually
  equivalent schemas. Combined text/graph admission and immutable context ownership
  are explicit; the strict projection helper and custom evaluator dispatch remain.

- Rust binding, source, dependency and operation-example views, with retained
  ownership, typed metadata and exact opaque content. The TypeScript facade adds
  matching keyed views and frozen metadata inventories.
- Lazy TypeScript exact object/array cursors and a scoped `ParsedDocument.toDraft()`
  editing path. `OwnedDocumentDraft.value` exposes mutable `EditableDocumentDraft`
  containers while preserving opaque exact values and readonly-friendly authoring
  inputs. A shipped inspection/editing example is compiled and exercised from the
  installed package.
- Default value-diagnostic `diagnostic_bytes` / `diagnosticBytes` budget (1 MiB
  retained UTF-8 strings), bounded pre-copy diagnostic construction, whole-problem
  truncation and explicit zero semantics. This adds an intentional prerelease Rust
  `Limits` struct field; `ValueOutcome` fields are unchanged.
- `EvaluationProgram::original_location_bounded` and `LocationBudgetExceeded` for
  custom evaluators that need original-coordinate admission before copying.


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

- Prepared contracts expose borrowed Rust `ResourceCompleteness` and a frozen,
  cached TypeScript `resourceCompleteness` getter. Existing custom evaluators default
  to Undeclared. Complete describes resources, not guaranteed value decidability.
  Retained service/editor examples now require Complete before replacing a snapshot.
- Declined optional partial planning again preserves the default evaluator's original
  located ResourceUnavailable refusal. Direct `evaluation_bounds()` retains planner
  errors; known defects, shared limits, cancellation and compilation keep their causes.

- The default evaluator can decide values independently of missing static resources
  within a qualified positive fragment, using shared-budget lower/upper evaluation
  and known-source upper diagnostics. Qualified partial contracts, including a bare
  missing `$ref`, now prepare ready; dependent values return `ResourceUnavailable`
  at validation. This is an intentional phase change and decision-coverage expansion.
  Hole-dependent nonpositive applicators and evaluated annotation/dynamic hazards
  refuse. Partial text admission counts serialized resource text and separately
  owned decoded strings, including escaped member names and nested opaque values,
  before constructing either projection. Complete-resource preparation retains its
  existing path.

- TypeScript operation inventories now interpret `tags` and `deprecated` along
  with the other metadata. Malformed values, including those on an unrelated
  operation, make the inventory throw an interpretation error; raw exact
  document access remains available.
- TypeScript authoring now accepts an unknown document member literally named
  `additionalFields` through the draft's additional-field map. Collisions with
  actual typed members are still refused.
- Abandoning an undisposed TypeScript editing scope keeps reachable converted
  exact leaves usable. Explicit scope disposal still releases every converted
  leaf, including removed or replaced leaves; retain leaves that must survive it.
- Exact object cursors use direct indexed access to each member, avoiding a
  repeated prefix walk. This is a structural work bound, not a measured speedup.
- Exact source coordinates share a lazy, bounded index within each retained
  document arena. Repeated metadata reads no longer rescan the complete source
  prefix; original byte offsets, lines, byte columns and pointers are unchanged.
- Exact duplicate-name queries use the existing source-ordered evidence: scalar
  queries are constant work and container queries use binary search, without a
  new retained index or changes to duplicate diagnostic ordering.
- Clarify that conformant dependency kind filters must be nonempty; this does not
  change draft interpretation behavior.
- Conformance diagnostics deduplicate repeated findings for the same original
  occurrence before count and pointer-byte limits. Identical schema text at
  different source positions remains distinct. Duplicate-member findings now
  locate each offending key token instead of its enclosing object; pointer/byte
  coordinates and retained counts under the existing limits can change.
  Rule evidence and truthful truncation remain independent of retained findings.
- TypeScript authoring failures add `invalid-field` and `duplicate-members`, plus
  optional original-source coordinates for parsed-to-draft conversion. Exhaustive
  switches over authoring failure codes need these cases.
- Dependency kind checks now report malformed kind lists as interpretation errors
  instead of silently treating them as absent or unmatched.
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
