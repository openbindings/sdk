# Changelog

## 0.2.0-alpha.1 (working draft)

- Known static reference defects in present carriers now participate in validity
  bounds. Positive contracts decide irrelevant defects, and return Undefined only
  when proven semantic holes have disagreeing concrete completions. Missing
  carriers and implementation limits retain distinct refusals. Unused malformed
  `$ref` siblings no longer disable valid targets; malformed `$id` siblings remain
  refused. Ten evaluator-kit cases and four corpus cases change as §5.2 and §7.4
  require.

- Default value failures now use `Diagnostics::Coarse`: a fixed value-free
  `message`, no problems, and incomplete diagnostics. Migrate
  `with_schema_details(true)` to `with_diagnostics(Diagnostics::Upstream)` and
  `includeSchemaDetails: true` to `diagnostics: "upstream"`; false/omission becomes
  Coarse. Upstream returns the library's first error, with library-bounded
  allocation and stack use (including possible stack overflow on deep values).
  Remove `max_problems`/`maxProblems` and `diagnostic_bytes`/`diagnosticBytes`.
  Rust `ValueOutcome::Fails` literals must now include `message`. Remove the unused
  `ValueProblemDetails::Truncated` / TypeScript `kind: "truncated"` variant; absent
  details represent unavailable facts, with no retired byte-budget marker.
- OBI-02/OBI-10 use library validity only and the fixed message
  `value violates the fixed normative schema`. Document findings retain original
  source locations, lose keyword wording, and may report several failing positions.
  OBI-02 descends through member/name and array-element constraints and local
  pointer references; independent container failures remain alongside child failures. OBI-10 stays at
  each checked schema node. Location work and findings caps never change evidence.
- Evaluator kit/corpus diagnostics migrate by case ID; verdict and refusal
  partitions are unchanged.

- S6d/e review fixes: make inspection string presentation injective, translate
  pointer references consistently, and select a collision-free synthetic root.
  Preserve original provenance for contained/supplied surrogate-name collisions.
  Pin the conservative unused-malformed-sibling limit pending S9; explicitly
  accept the library's encoded-leading-slash refusal under §7.4.

- Integrate S6d/e: move bounds planning, paired compilation and guarded source
  mapping into private evaluator modules. Remove `ResourceCompleteness`, its
  Rust/TypeScript accessors, `SchemaRequest::evaluation_bounds`, `EvaluationBounds`,
  `LocationBudgetExceeded` and `EvaluationProgram::original_location_bounded`
  without replacement product APIs. The strict projection exposes read-only
  `entry_uri()` / `resources()` accessors. Ready means prepared, not decidable (EV-9).
- Reference inspection now has `ReferenceRefusal` / `ReferenceRefusalReason`,
  including Cancelled. Ordinary resolution uses documented library APIs, recording
  inspection-only retrieval and original pointer identity; no acquisition occurs.
  Preserve §7 document embedding and OBI-13 exact comparison. Follow library URI
  normalization and encoded-name refusal under §7.4, with approved per-case ledger
  entries. Supplied normalized collisions remain `ambiguous-resource`.

- Recheck cached Rust document conformance at every proof-requiring Wasm entry,
  including raw receiver substitution, retention, transfer and publication. Keep
  TypeScript top-level text reads field-local so malformed siblings do not hide
  readable fields or absence.

- Require a validated document for normative value contracts and operation invocation
  across Rust, Wasm and TypeScript. Add infallible validated views and top-level
  metadata/schema reads; preserve exact UTF-16 with Rust `JsonStringRef` and JavaScript
  strings. Add TypeScript operation input/output reads with independent ownership.
  Parsed editor reads remain fallible. Re-express nine evaluator-kit cases through
  supplied resources with unchanged refusal outcomes; migrate two unreachable OBI-12
  instance cases to core rejection tests, with complete case-ID accounting.

- Caller-owned preparation: remove the core preparation cache, Rust
  `ValueContractOptions` / `value_contracts_with_options`, the Wasm cache argument
  and TypeScript `cacheCapacity`. Every explicit preparation compiles anew;
  retain a ready `ValueContract` for reuse. Add `operation::PreparedOperation`
  through `Invoker::prepare` / `prepare_with_control`, with `invoke` /
  `invoke_with_control` on the handle taking `operation::Attempt`: `new()`,
  `ordered(&[..])`, `binding(key)` and `.with_context(ctx)` carry binding choice
  and opaque context without repeating the operation name. Foreign exact bindings
  return `ERR_OPERATION_NOT_FOUND` before provider work. Both declared contracts
  are prepared once; selection, context and cancellation remain per attempt.
  Failed or cancelled preparation leaves no handle. Plain invocation keeps
  `Request` and prepares lazily per attempt. Remove `ValidationReport.policy`;
  retain specification identity,
  conclusion, evidence, findings and truncation.

- Vocabulary pass (V), a breaking working-draft migration without compatibility aliases:

  | Previous surface | Replacement |
  | --- | --- |
  | Rust `DocumentBuilder`, `OperationExample` | `Document`, `Example` |
  | `assess_document`, `assess_document_with_limits`, `ParsedDocument::assess`, `assess_body`, Wasm `assess_bytes` / `assessBytes` | `validate_document`, `validate_document_with_limits`, `validate`, `validate_body`, `validate_bytes` / `validateBytes` |
  | `ConformanceReport`, Rust `DocumentAssessment`, TS `Assessment` | `ValidationReport`, `DocumentValidation` |
  | TS `assessDocument`, document `assess()`, document `validate()` | `validateDocument`, `validate()` (evidence), `validated()` (proof) |
  | `RejectedSynthesis.assessment`, discovery `assessment` payloads | `validation` |
  | `PreparedContract`, `ContractPreparation`, `NoContract` / `no-contract` | `ValueContract`, `ValueContractPreparation`, `NoValueContract` / `no-value-contract` |
  | TS `ContractOptions`, `contracts()`, ready `contract` | `ValueContractOptions`, `valueContracts()`, `valueContract` |
  | Wasm `WasmContracts`, `contracts`, `take_contract` / `takeContract` | `WasmValueContracts`, `value_contracts` / `valueContracts`, `take_value_contract` / `takeValueContract` |
  | `Side`, directional `side` parameters/accessor | `Direction`, `direction` |
  | TypeScript error code `invalid-side` | `invalid-direction` |
  | `Satisfied` / `satisfied`, TS evidence `assessed` | `Met` / `met`, `report` (proof `validated` stays) |
  | OBI-06/08 `missing-target`; OBI-07 `missing-target` | `unknown-operation`; `unknown-source` |
  | `operation-contract-invariant` | `value-contract-invariant` |
  | Rust discovery `RequestFailure`, `FailureKind`, `TransportFailure`, `BodyFailure`, `failure` payload | `RequestError`, `ErrorKind`, `TransportError`, `BodyError`, `error` payload |
  | Rust `max_document_bytes: i64`; zero selects default | `usize`; `Default` selects 1 MiB; zero and `usize::MAX` refused. TS omission selects 1 MiB; zero refused. |
  | String CORS options and empty-string omission | Rust `AllowOrigin::{Any, Omit, Null, Exact(String)}`; TS `"any" | "omit" | "null" | { origin: string }`; default any, exact origins canonical |
  | `ClientOptions::discover_with_timer`, always-refusing `discover_with` | Free `discover(origin, &options, &control, &timer, transport)`; native `Client::discover` stays |


- Evaluator correctness. A supplied resource, nested `$id` or retrieval alias that
  claims a packaged standard identity is now an identity conflict instead of replacing
  it, so a completion can no longer invert a verdict established with a missing
  resource. In the maintained regress fork, quantified lone-surrogate atoms such as
  `\ud83d?` accept zero repetitions per ECMA-262 (checked against Node's RegExp), and a
  budget interruption leaves the backtrack stack intact instead of panicking in debug
  builds. Pattern-compilation refusals name the authored `pattern` or
  `patternProperties` location.

- Require HTTPS for discovery except normalized literal 127/8 and `::1`; native
  and portable paths allow ten redirects and refuse HTTPS-to-HTTP hops. Fetch
  follows browser policy and checks its final URL. Safe native construction refuses
  A -> B -> B credential replay caused by tower-http restoring original headers
  while reqwest strips credentials only on origin changes. Default to a 30-second
  whole-attempt timeout and publication CORS `*`; remove reqwest gzip, brotli,
  deflate and stream features. Add Rust `AttemptTimeout::{Default, After, Unbounded}`,
  `duration()`, `ClientOptions::timeout`, `DiscoveryTimer::{now, sleep_until}`,
  `ClientOptions::discover_with_timer`, `DEFAULT_TIMEOUT_SECONDS`, `MAX_REDIRECTS`,
  `native::Client::with_delegated_policy`, `DiscoveryOutcome::Timeout` and
  `FailureKind::{RedirectPolicy, Decode}`; native `Client::new` now takes a builder.
  Add TypeScript `DiscoveryOptions.timeoutMs`, status `"timeout"` and kind `"redirect-policy"`.

- Whole-document assessment no longer declines for escaped lone surrogates or schema
  depth above 256. `assess_document_with_limits` accepts caller-supplied admission
  limits; admission precedes version selection. The default evaluator refuses
  unsupported string interpretation only when reached, per evaluation scope.
  Assessment findings use `input-limit` for admission limits; `representation-limit`
  and `schema-depth-limit` are no longer emitted by assessment. CONFORMANCE-06 changes
  from undetermined to conformant because its description's lone surrogate no longer
  gates assessment.

- Finish the source-inspection, synthesis and invocation role definitions. Providers
  move `BindingInvoker::preflight` and its future type to optional `BindingPreflight`;
  callers opt in through `preflight_with_provider`. Invoke-only and local providers
  remain supported. Provider errors using any of the six operation-owned codes become
  `ERR_EXECUTION_FAILED`; invocation retains a `reserved-provider-code` finding.
  Unsupported explicit operation bindings use `ERR_BINDING_NOT_FOUND`; errors show codes.
  Synthesis compares numbers by exact value, allows unbound operations and optional
  dependency attribution. Supplied keys must exist; no one-to-one mapping is required.
  Inspection framing refuses unknown non-`x-` members. `InspectionLimitation` gains
  `additional_fields`; existing struct literals must supply the map.

- Duplicate-member detection and same-document JSON Pointer resolution now run in
  linear time. An object repeating its keys and a document with many schema
  references, each under 1 MiB, previously took several seconds. Results, source
  order and duplicate coordinates are unchanged.

- Extract the kind-specific OpenAPI implementation into
  [openbindings/openapi](https://github.com/openbindings/openapi). The SDK retains
  the source-inspection, interface-synthesis and invocation roles, their provider
  traits, checked caller APIs, generic examples and tests. Historical adapter and
  role-loop records are archived outside the product repositories.

- The bundled resource resolver now gives unique contained declared IDs priority
  over supplied name associations, with conservative refusal of competing canonical
  carriers reached through aliases, pointers or direct applicators. This intentionally
  expands contained-target admission and tightens competing-carrier routes. Supplied
  catalog contents and original retrieval provenance remain intact; public API shapes
  are unchanged. Previous conservative refusals are not reclassified as normative bugs.

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
- Partial-resource bounds now support hole-dependent `oneOf` through finite
  dual-polarity projections, with original-location, cardinality-neutral oneOf
  failure summaries. Correlations between unknown predicates remain undecided.
  This intentionally changes qualified oneOf setup refusal to ready; dependent
  values still return ResourceUnavailable. Other influence exclusions remain.
  Two new generated-plan admission limits count duplicated nodes/edges across
  both closed programs and recover the original strict missing-resource evidence
  when the default fallback declines. Private planner proofs retain their codes.


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

- Declined optional partial planning preserves the default evaluator's original
  located ResourceUnavailable refusal. Private proof tests retain planner errors;
  known defects, shared limits, cancellation and compilation keep their causes.

- The default evaluator can decide values independently of missing static resources
  within a qualified positive fragment, using shared-budget lower/upper evaluation
  and known-source upper diagnostics. Qualified partial contracts, including a bare
  missing `$ref`, now prepare ready; dependent values return `ResourceUnavailable`
  at validation. This is an intentional phase change and decision-coverage expansion.
  Other hole-dependent nonpositive applicators and evaluated annotation/dynamic hazards
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
