# Changelog

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

- Bound reference reports to the same 8 MiB generated-pointer allowance as
  conformance findings. Stop at the first omitted entry with
  `reference-pointer-bytes`, including located targets and refusal locations.
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
independent. This entry describes the current feature-branch implementation;
[the alignment checkpoint](docs/alignment-checkpoint.md) records its validation
and remaining work. Earlier development APIs have no compatibility guarantee.

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
