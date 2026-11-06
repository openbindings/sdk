# Changelog

## 2026-11-06 selective pull adoption (unreleased)

- Add `PullResult::keep` to validate and retain selected added bindings with their
  added operations, binding data `uses` and transitive schema-reference targets.
  Resolve `$id`, anchors and references through existing entries in one full
  result inspection; visit each reached node and edge once, including cycles.
- Add `RelationshipError::KeptBinding` for keys absent from the report's `added`.
  Keep the original immutable, leave proposals unapplied and omit additions
  needed only by proposals. Share edit construction with `validate_for`.
- Cover selective adoption, the pinned report fixtures and default-limit keeps
  of a near-30-MiB result and 5,000 operations. Update the consumer and guides.
  No dependency, default limit or TypeScript API changes.

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
