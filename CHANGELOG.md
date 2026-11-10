# Changelog

## 2026-11-10 objective review fixes (unreleased)

### Changed

- Schema comparison visits only the `allOf` branch pairs that can matter, charges
  each one to the work allowance and stops once the conjunction is retained.
- Schema comparison resolves an empty `$ref` as it resolves `#`, with or without a base.
- TypeScript sends bytes past the byte limit, and text with more UTF-16 units
  than it, into Wasm only as the prefix that establishes the refusal; the
  refusal and its byte coordinates are unchanged.
- The byte-limit validation finding reads "the document exceeds the admission
  limit of N bytes", the form the node and depth findings use, without the
  input's size.
- Source pull checks each report label that names an entry of the input document
  (a binding entry's `references`, and an omitted entry's `existing`,
  `alsoIdentifies` and `through`) for its form and its target, adding
  `RelationshipError::ReferenceTarget` and `OmittedTarget`.
- Operation invocation admits an empty `operation` or `binding` name, as the
  operation-invoker interface does; invocation, preflight and
  `DefaultOperationInvoker::prepare` end with `ERR_OPERATION_NOT_FOUND` or
  `ERR_BINDING_NOT_FOUND`. Selection entries and binding-invoker requests keep
  their non-empty rule.
- OBI-11's finding names a schema `$id` at an OBI position, the specification's phrase.
- Docs: preflight's no-prediction result is `None`; cancellation drops the provider
  future at the output's next poll or drop; the Rust guides state the Worker
  request rule; the runtime-qualification observations match the current harness.

### Removed

- `OperationPreparationFailure::Admission`: preparation resolves every name, so
  nothing produced it.

## 2026-11-09 public API consensus batch (unreleased)

### Changed

- Use default preparation limits and no cancellation in Rust
  `Schema::prepare(&evaluator, &external)`; use `prepare_with_control` for explicit
  limits or cancellation. Keep the evaluator trait and invoker unchanged.
- Document fail-closed routing for providers without routing knowledge, the
  diagnostic field/accessor rule and bounded carrier encoding.
- Add `CheckRejection::value()` to borrow its exact admitted object.

- Lift `InvocationSupport` from `DefaultOperationInvoker` construction,
  configuration and schema preparation; keep it on invocation and preflight.
- Add `InvocationError::binding_not_found()` and use it in adapters and guides.
- Replace `PreflightOutcome` with `Option<ContextChallenge>`; `None` means no
  prediction. Keep required `handle_preflight` and document its one-line body.
- Show fixed supported versions and preparation-failure variant names/origin
  presence in `Debug`, while keeping document and caller payloads hidden.
- Add Rust parse-error `line()`/`byte_column()` and TypeScript `line`/`byteColumn`,
  computed only on failure.
- Hide core's internal schema-failure conversion and remove unreachable
  `invalid-schema` classification arms. Document the default evaluator's shared
  internal implementation and third-party public evaluator contract.
- Return `Result<bool, ComparisonFailure>` from schema identity comparison.
- Add `Document::retained_bytes()` and TypeScript `Document.retainedBytes()` to
  estimate current shared snapshot storage, including built caches, on demand.
  Exclude allocator overhead and Wasm memory growth; document later cache growth.

### Removed

- Remove Rust `PreparedSchema::evaluate_json` and `evaluate_json_with_control`;
  parse with `JsonValue::parse` or `parse_with_limits`, then `evaluate` or `explain`.
  TypeScript `evaluateJson` retains its admission limits, messages and behavior.
- Remove `ConformantDocument::references` and `references_with_resources`; call
  them through `proof.document()`.
- Make `VersionRefusal::new` crate-internal; get refusals from validation and
  classify standalone declarations with `check_version`.
- Remove clone-only `to_json` from `ContextChallenge`, kind-description and
  source-pull carriers and `CheckRejection`; borrow `value()` or clone explicitly.
  Keep bounded invocation error and request encoders.

- Remove `PreflightOutcome` and schema comparison's `Identity` enum.
- Remove `PreparedSchema::into_validator`, `SchemaInput::standalone` and
  `ExternalSchemas::with`; use the evaluator contract and explicit input/resource constructors.
- Remove public invocation-carrier `from_json_ref` decoders; use `from_json(&JsonValue)`
  and retain a borrowed frame with `to_owned()` when needed.


## 2026-11-08 newcomer docs and omitted-entry accessors (unreleased)

- Fix failure diagnostics, Node worker and disposal examples, and discovery client setup in the newcomer docs.
- Add borrowed `OmittedEntry` accessors for `item`, `existing`, `also_identifies`,
  `offered`, `through` and `identifier`, preserving exact values and absence.
- Add a document-to-typed-result walkthrough with an in-process binding adapter,
  preparation, failure handling and Serde output decoding. Verify its exact
  source and external example crate through the Rust snippet verifier and CI.
- Correct evaluator routing, limits, discovery and package documentation; remove
  duplicated migration status and capability detail.
- Remove the historical alignment checkpoint (924 files of review and
  qualification evidence) from `docs/`, so `docs/` holds only material for
  people using or maintaining the SDK. It remains in Git history at commit
  `d29864e`.
- Correct the documented `InvocationSupport` requirement. Its bound is unchanged:
  runtime operation/binding address variants share one entry point, so a pure
  conditional relaxation needs a separately designed public API.

## 2026-11-08 TypeScript report hand-off (unreleased)

- Serialize reports from the Wasm engine in one Serde pass instead of
  assembling nested JSON text per record, and translate each field name once
  per decoded report. Inspecting 10,000 references in Node takes about 28 to
  61 ms instead of 125 to 183 ms; a 4,096-finding validation report decodes in
  about 14 ms instead of 44. Public types, values, field order and exact
  pointers are unchanged, and returned objects stay independent.

## 2026-11-08 reference resource roots and report text (unreleased)

### Added

- Add `Reference::resource_root` and TypeScript `resourceRoot`: the original
  location of the schema beginning the site's resource, absent exactly for the
  document resource. This is a location, not a resolved base.

### Changed

- Rename the unreleased reference allowance code to `reference-report-bytes`.
  One 8 MiB allowance now charges all retained generated and source text in each
  reference entry, including every origin's source URI, resource-root pointers,
  spellings, keywords, refusal codes/messages and resolved unavailable-resource
  URIs. Admit the whole entry before copying its text or spelling a missing URI.
  Stop at the first omitted entry with a location-free `LimitExceeded` limitation.
  The allowance bounds text, not total heap usage.

## 2026-11-08 application plain HTTP discovery policy (unreleased)

- Add `ClientOptions::allow_http` and TypeScript `DiscoveryOptions.allowHttp`,
  both false by default, to admit plain HTTP origins and HTTP-to-HTTP redirects.
  Plain HTTP exposes requests, credentials and documents to network observation
  and modification. HTTPS-to-HTTP hops remain refused, with other admission,
  redirect-limit and credential rules unchanged.
- Add `ClientOptions::endpoint` and an optional policy argument to TypeScript
  `discoveryEndpoint`; the Rust free `endpoint` keeps the default policy.
  Keep `origin-policy` for refused plain-HTTP origins and state the policy
  refusal without API advice in its message.
- Apply the choice in native and portable Rust and the Fetch facade. With the
  option enabled, hidden Fetch redirects return `redirect-policy` without
  automatic following. Browser and Worker restrictions still apply.

## 2026-11-08 conformance finding facts (unreleased)

- Add OBI-02 `keyword`, published `schema` origin and optional `ValueProblemDetails`
  to Rust and TypeScript findings. Required members get individual findings;
  disallowed members are identified by source pointer without copied document data.
- Keep failed `anyOf` and `oneOf` at the applicator without branch details.
  Core conformance continues to check documents only.
- Guard the pinned 0.2 schema's measured maximum fact tuple of 199 serialized JSON
  bytes (198 in TypeScript) with an exhaustive schema test. Keep the existing
  4096-finding and 8 MiB source-pointer caps without a runtime fact-size check.
- Replace conformance `input-limit` with `byte-limit`, `depth-limit` and `node-limit`,
  preserving parser offsets. Move Rust and TypeScript API advice from finding
  messages into documentation. This changes visible codes before publication.

## 2026-11-08 compact reference resolution (unreleased)

- Resolve a relative reference against a named base through validation's
  compact identifier walk instead of spelling the base for every reference.
  Inspecting a 1 MiB `$id` with 10,000 fragment references takes about 24 ms
  natively instead of 14 s. Reports, their order and completion are unchanged.
- Absolute references and references without a named base keep the string
  resolver. The environment builds one immutable inventory of its own names on
  first need; each request's query inputs are released when the request ends.
- Preparation charges a resolution for its own reference text, plus a missing
  name only when the result retains one. Many distinct fragment references
  under a long base now prepare under default limits instead of refusing with
  `preparation-sdk-work-units`. No allowance value changed.
- Guard per-reference resolution work with a deterministic byte count at 1 KiB,
  64 KiB and 1 MiB bases, and freeze the original resolver's reports for five
  reference shapes with and without supplied resources.

## 2026-11-07 draft reference inspection (unreleased)

- Inspect drafts with missing or malformed `openbindings` declarations under
  0.2's rules, matching validation. A complete traversal is complete without an
  assumed-version limitation. Keep refusal of well-formed unsupported versions
  and duplicate member names, and retain the existing pointer allowance.
- Inspect references beyond 256 schema levels, within document admission and
  the existing identity and pointer allowances. Keep preparation's separate
  depth guard. Qualify deep targets and identity refusal on a 256 KiB stack.
- Qualify matching reference order, targets, dynamic lookup and unresolved
  results with and without supplied resources in Rust and TypeScript.

## 2026-11-07 compiled decimal peak selection (unreleased)

- Admit over-estimate acyclic bounded decimal values when one comparison's
  conservative peak scratch fits the existing reservation. Include the longest
  value number token and schema comparison bound; nearly 16 MiB exponent
  spellings still select Original. Previously admitted values pay no new scan.
- Preserve Original accounting, defaults and selection for division, number and
  composite literals, uniqueness and recursion. Qualify decisive agreement with
  independent Original diagnostics for hostile values and run the decimal
  allocation proof in the verifier.

## 2026-11-07 compiled expansion selection (unreleased)

- Generate boundary-biased schemas and values at default limits, comparing
  evaluate and explain verdicts with an asserted compiled-route comparison floor.
- Add five standalone cargo-fuzz targets, stable committed-corpus replay in the
  verifier, and a pinned weekly/manual workflow that preserves crash artifacts.

- Bound reference reports to an 8 MiB allowance for retained generated and source
  text in entries. Stop at the first omitted entry with `reference-report-bytes`,
  including all origins, reference text and refusal text.
- Pin regex 1.13.1 exactly and qualify dense admitted customer patterns on empty
  strings and member names against the bounded-backtracker search panic.
- Run the compiled lane's release-only stack boundary tests on every host; they
  had been ignored in debug builds and selected by no release command.
- Qualify compiled depth fallback through the public Wasm API with shared Node,
  browser and Worker cases, and replace obsolete stack-setting documentation.
- Bound acyclic compiled schema expansion at preparation using the recursive
  proof's existing 100,000-application ceiling. Larger graphs take the Original
  lane with reason `acyclic-finite-expansion-profile` before compiler entry.
- Count repeated reference and applicator paths with a saturating memoized fold.
  Shared definitions remain shared during preparation.
- Before compiled evaluation, bound weighted schema expansion times value node
  count. Over 40,000,000 applications selects Original before backend entry.
  Include enum choices, use the largest disjoint sibling property or prefix-item
  weight, and retain overlapping paths. Recursive admission, scratch charges,
  public limits and defaults are unchanged.

## 2026-11-07 compiled scratch selection (unreleased)

- String, boolean and null literals no longer inflate the acyclic compiled
  scratch estimate. Recursive admission retains its existing weights.
- Composite values over the estimate still enter when every number fits i64 and
  their remaining scratch sources are integer typing and i64 bounds. Values
  within the estimate are not scanned. Limits and exact arithmetic are unchanged.
- Run the compiled lane's allocation proof in the verifier.

## 2026-11-07 clearer carrier errors (unreleased)

- Carrier errors name the carrier when several share a field name, such as
  `check/description` and `existing/message`, so the same text no longer covers
  different places. The error types are unchanged.
- `PullRequest::new` takes `impl Into<String>`, as the other request constructors
  do, so a `&String` works.

## 2026-11-06 binding data needs (unreleased)

- Apply interfaces `3e3dc453a93b611a8e9ea1813ec63a2216830f45`; retain specification
  `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`. The interfaces revision is an open
  proposal and will be repinned after squash merge.
- `uses` names only additions needed by binding data and is not transitive.
  Remove report and core schema-reference closure checks. Keep label checks and
  prospective document conformance. Schema proposals carry `{schema}`; remove
  their `uses` accessor and treat that member as unknown metadata.
- Refresh publication fixtures, scale tests, selective-adoption examples and the
  adapter guide. Callers keeping part of a pull follow schema references and each
  reached added binding's operation and `uses`. No default limit changes.

## 2026-11-06 interface repin (unreleased)

- Apply interfaces `5006884a39c055103fad49e7c47b3141726f8dd8`; retain specification
  `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`.
- Align pull carriers, removal proposals, schema proposal dependencies, multiple
  losses, unassessed entries and core reference closure with the merged drafts.
- Add kind pull and invocation disclosures and apply the comparison normalization,
  resource reference, retained definition and absent-slot cases.
- Index core uses closure and batch proposal inspection under the relationship
  work ledger. Remove obsolete union-merge and unrepresentable-removal failures;
  expose the local catalog error through Display and its wrapped error source.


## 0.2.0-alpha.1 (working draft)

This first release remains unpublished. The SDK and specification versions are
independent. This entry describes the current feature-branch implementation.
Earlier development APIs have no compatibility guarantee.

### Security

- Replace input-keyed hash tables with ordered storage, including JSON member
  lookup, uniqueness grouping and schema indexes. Parsing and lookup require no
  entropy and retain exact keys, duplicate locations and first-occurrence lookup.

### Added

- Exact OpenBindings documents with immutable edits, owning domain entries and a
  separate `ConformantDocument` proof. Edits produce a new revision; existing
  owners retain their original bytes and evidence. Unknown fields, opaque data,
  numeric tokens and escaped strings survive inspection and supported edits.
- Separate input, version and conformance results. OBI-01 through OBI-14 have
  per-rule evidence and bounded original-location findings. Parsing alone does
  not establish conformance, and conformance does not prove an author's claims
  about bindings, examples or availability.
- Original, proof-owned `Schema` values for declared operation inputs and outputs,
  with explicit external resources and a generic `SchemaEvaluator` boundary.
  `PreparedSchema` owns its validator; preparation failure is separate from
  `Satisfies`, `Fails`, `Undefined` and `NoVerdict` value outcomes. Core performs
  no acquisition and requires neither a particular evaluator nor `Send`/`Sync`.
- An optional JSON Schema evaluator with exact numeric predicates, explicit
  capability limits and cooperative cancellation. Its prepared original-node
  engine retains schema environments and static reference/pattern readings,
  tracks evaluated annotations, and uses explicit applicator frames and bounded
  recursive reasoning. Known undefined effects remain distinct from missing
  resources, unsupported capabilities and interrupted execution.
- Original-schema explanations for one concretely proved failure, with exact
  source and instance locations. Verdict and optional explanation consume one
  cumulative allowance. Diagnostic interruption preserves an established
  verdict; explanations remain explicitly incomplete.
- Optional Rust binding and operation invocation interfaces over explicit borrowed
  providers. Binding invocation neither selects a binding nor validates operation
  values. Operation invocation resolves keys/aliases, applies caller selection
  and checks each value against its declared schema before forwarding it.
  Prepared operations support repeated calls without retaining a provider.
- Separate advisory preflight, opaque context and exact context challenges.
  Streaming invocation preserves prior output when a later failure occurs;
  cancellation promises neither rollback nor that external effects have stopped.
  Applications own provider choice, context acquisition, approval and retry.
- Optional kind-description and source-checking interfaces. Capability claims remain
  per operation, kind identity is exact, and checking requires an explicit call.
  Kind-owned data and native provider failures retain their separate meanings.
- Optional source pulling with original-document requests, additions, reports,
  proposals and explicit caller adoption. Local checks establish mechanically
  knowable relationships and prospective document conformance; they do not prove
  kind-owned fidelity, offered-schema identity or opaque dependency closure.
  Pulling never applies proposals or retries automatically.
- Optional schema comparison using the pinned profile's identical mode, explicit
  roots/bases/resources and exact values. Equality means profile identity, not
  general schema equivalence. Unsupported comparisons return no identity verdict.
- HTTP discovery and publication as an independent companion, with bounded
  decoded bodies, explicit optional deadlines, redirects and distinct gated,
  absent, invalid, inconclusive and transport outcomes. Publication retains a
  conformant snapshot. Authentication and deployment policy remain with the host.
- A TypeScript facade over the shared Rust/Wasm engine, organized around exact
  values, documents, schema preparation/evaluation and Fetch discovery. Disposable
  owners and derived views preserve their lifetimes. Rust interface APIs are not
  exposed as TypeScript interface APIs.
- Source and packaged consumer checks, current Rust API references, required
  conformance/evaluator corpora, actual browser probes and reproducible Wasm
  builds. Cargo and npm publication guards remain enabled.

### Changed

- Call binding invocation, source pulling and kind queries directly on their
  implementers through SDK-owned checked extension methods. Adapters implement
  `handle_*` hooks; custom limits are call arguments. `OperationInvoker` defines
  operation invocation and `DefaultOperationInvoker` supplies resolution, binding
  selection, validation and prepared reuse. `Invocation::new` supports third-party
  implementations. Source pulling and kind queries expose `PullError` and `KindError`.
- Name preparation's cooperative counters `max_sdk_work_units` and
  `max_sdk_reservation_bytes`, with matching `preparation-sdk-*` limit codes.
  Their scope is explicit SDK work/storage sites, not every dependency operation
  or allocator request. No aliases for the earlier unpublished names remain.
- Apply specification `c1419c9fc3ae5a00c2b83da710c11bf67dab7934` and current
  interfaces `5815fb106bddd2c85ff4e93e0077570b3f6b9e74`. Source and binding payloads
  use `data`. Current contained-resource reference rules, derived schema and
  qualification inputs move together with those pins.
- Align public vocabulary and organization with schemas, document conformance,
  context, invocation, kind description and source pulling. Replace old value
  schema and detached inspection/synthesis APIs with those domain boundaries.
- Rust `NoVerdict` code/message fields use `Cow<'static, str>`. Fixed SDK
  limit/cancellation text and clones require no heap allocation; dynamic owned
  strings move into the record. Serialized fields remain strings.
- TypeScript result unions and corresponding serialized Rust results use explicit
  status discriminants. Exact admission refuses unsupported ordinary-value
  conversions instead of silently changing values.

### Removed

- Obsolete inspection/synthesis companions, shared legacy interface records and
  their consumers. No compatibility adapters are retained.

### Remaining alignment work

- Complete the pattern compiler's structural admission, single-build ownership
  and measured resource qualification; retain full immutable pattern programs
  during original-schema preparation.
- Finish physical-stack qualification, force the original engine through all
  gates, switch the default and remove the outgoing projection/compiler path.
  The ordinary evaluator still includes that path at this checkpoint.
- Complete matched performance, clean-package/reference/all-host qualification
  and the final specification-to-code audit. Bounded passing increments do not
  establish whole-SDK completion; detailed capability limits remain in
  [CAPABILITIES.md](CAPABILITIES.md).
