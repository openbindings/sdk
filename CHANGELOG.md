# Changelog

## 2026-11-16 comparison profile amendment (unreleased)

### Changed

- The interface companions (context, invocation, kind description, schema comparison, source pull) and their
  vendored fixtures are pinned to interfaces revision `8be6f6837f90da7091e5496a1686e34bf3b57c15`, which amends the
  OB-2020-12 schema-comparison profile 0.1 in place and drops the unsatisfiable merge from the source-puller's list
  of comparisons that cannot finish. The `APPLIED_INTERFACE_REVISION` constants change; the profile identifier and
  version do not. The comparison corpus grows to 271 groups and 44 exact cases.
- Breaking: an unsatisfiable merge (an empty `type` or `enum` intersection, or conflicting `const` values) is no
  longer an error. It normalizes to `{"not":{}}` at its own position, the compared schema included, so identity and
  directional comparison answer where they returned `UnsatisfiableMerge`. `ComparisonFailureKind::UnsatisfiableMerge`
  is removed.
- Breaking: the exact empty schema (`false`, or `{"not":{}}` after annotations are stripped) no longer keeps `$defs`
  by itself, so a closed object reached through `$ref` is identical to the same object inline. Other retained content
  still keeps definitions.
- Breaking: some verdicts change. A flowing `const` or `enum` now proves Compatible when the receiving schema accepts
  every value that can flow, checked exactly through `properties`, `additionalProperties`, `items` and `anyOf`; with
  both keywords only the values both allow flow, and none flowing is Compatible where the profile's boundary rule
  allows. Six corpus groups move from Indeterminate to Compatible. Whether an authored value that the receiver refuses
  makes the position Incompatible is now judged per value: a keyword that cannot refuse that value (Top-equivalent, or
  for another JSON type) no longer withholds the failure, so some Indeterminate answers become Incompatible.
  Outside-profile, foreign-dialect, `oneOf` and retained `allOf` boundaries are unchanged. The literal proof's work
  can move the cutoff of an explicitly configured comparison allowance.

## 2026-11-16 repairs (unreleased)

### Added

These follow the 2026-11-16 decision panel's strong consensus on the review's API questions.

- TypeScript registration refusals (`new ExternalSchemas(...)`, `resources.with(...)`) carry the limiting resource
  as `details.resource`, the strings preparation refusals already use; the code stays `resource`.
- Constants for the interface operation keys, each in the crate that implements the role: `LIST_KINDS`,
  `DESCRIBE_KIND` and `CHECK_SOURCE` (kind description), `PULL_SOURCE` (source pull), `INVOKE_BINDING` and
  `PREFLIGHT_BINDING` (invocation). A test holds each to the pinned interface document.
- The role crates that re-export `Context` and `ContextChallenge` also re-export `ContextShapeError` and
  `ChallengeShapeError`.
- TypeScript `PreparedSchema.explainJson(text | bytes)`, the sibling of `evaluateJson`: it admits JSON and explains
  it in one call, with nothing to dispose.

### Fixed

- Schema comparison could answer Compatible for a schema selected under a member that is not a keyword of its
  document's dialect (`/schemas/A` in a standalone draft-07 root), because the dialect walk stopped at that member
  and read the position as 2020-12. A selected position now takes its enclosing resource's dialect through any
  members; positions in an OBI document are still read as 2020-12.
- An established failure is no longer lost when an allowance runs out: a position that has definitely failed (an
  array past `maxItems`, say) completes at once instead of visiting its items, so under the Workers guide's limits an
  array of `$ref` items answers `fails`, where it answered no verdict for memory. Unknowns never erase a definite
  failure, as before.
- An explanation that cannot finish keeps the failing position's own problems: `explain` on that array reports
  `maxItems` with `problemsComplete: false`, where it reported no problems. Explaining such failures is also far
  faster, since children that cannot add a retained problem are not visited.
- Schema comparison resolves a relative `$ref` inside an embedded resource whose own `$id` is relative, where it
  answered `MissingReference`.
- The TypeScript Worker and service examples follow the Workers guide: a streamed body limit, its JSON, registration,
  preparation and evaluation limits, and no owner held across an `await`.

### Changed

- Breaking: `InterfaceIssueKind` (schema comparison) and `OperationPreparationFailure` (invocation) are
  `#[non_exhaustive]`; a match outside the crate needs a wildcard arm.
- The TypeScript README says at the top that SDK calls cannot be interrupted and that untrusted input needs
  `evaluationSdkWorkUnits` and JSON limits; the first-use examples set both. The SDK still sets no default work limit
  (the owner's ruling, 2026-11-16).
- The TypeScript README documents that `set` on an existing path replaces the whole member, findings whose status is
  `met`, top-level `schemas` with `#/schemas/` references, and explaining JSON text. The Workers guide says its
  evaluation limits select the SDK's own lane and how a Worker answers a no-verdict. The comparison docs say a
  detached copy that re-declares the document's identifiers is ambiguous.

## 2026-11-15 review repairs (unreleased)

### Fixed

- A supplied draft-07 schema whose root `$ref` points into its own `definitions`, the common shape of generated
  draft-07 files, evaluated to undefined for every value. Draft-07 ignores the siblings of `$ref` when evaluating, but
  they stay reachable by a pointer; the evaluator now finds them and gives the draft-07 answers, as the bundled
  `jsonschema` does, while still ignoring sibling assertions and identifiers. 2020-12 and OBI documents are unchanged.
- A decimal-spelled numeric `enum` could exhaust the default evaluation reservation depending on the order of the
  values it was checked against (1,440 choices passed ascending and failed descending), because exact comparisons kept
  their scratch charged after releasing it. Comparison scratch is now returned when each comparison ends.
- Registering external schemas charges the preparation allowance for the storage registration holds, as preparation
  has since the previous change, where it charged estimates. The smallest allowance that registers 3,000 supplied
  records falls from 19.7 MiB to 9.6 MiB in Wasm, and from 21.7 MiB to 11.6 MiB natively, where the charge now equals
  what a counting allocator sees; registration is 17 to 22 percent faster. Defaults and the limit's meaning are
  unchanged.
- The Workers guide's recommended settings now admit the workloads it says they admit: its supplied-records recipe
  had been refused at its own 8 MiB, because the measurement runner registered with default limits. The runner now
  registers with each row's limits and measures the guide's whole recipe (parse, validate, register, prepare,
  evaluate) with the guide's own option objects, which a test keeps equal to the guide's code block. The guide starts
  at 16 MiB for 3,000 supplied records or 16,000 links, and gives the 49,000-link chain 8 MiB of evaluation storage.
- Schema comparison could answer Compatible for a standalone draft-07 or draft-04 root, for a value that flows and
  is refused: as the context's own document without a base, or as a separately parsed copy, the root was read as
  2020-12. A schema outside an OBI document now keeps the dialect JSON Schema assigns it on every route, with or
  without a base; schemas inside an OBI document are still read as 2020-12.
- Kept `$defs` no longer hides an incompatible beside a union: kept definitions are disregarded by every directional
  rule, as the profile says.
- A keyword that applies only to a type the flowing schema does not allow (`maximum` beside `"type":"boolean"`, say)
  no longer withholds an incompatible verdict from the `const`, `enum` and finite-restriction rules.
- The comparison docs say that a schema equal to but not the same node as its document is a detached input, label
  the recognition of an OBI document by its `openbindings` and `operations` members as SDK policy, and give the types
  `OperationComparison::required()` and `provided()` return.
- The default ceiling on one uncancellable pattern search no longer refuses ordinary patterns on ordinary input.
  The charge was the cost of every automaton state at every position; it now bounds what the engine actually does: a
  start-anchored pattern with a finite longest match is charged for at most that many bytes, an unanchored pattern
  searches a bounded prefix and answers a match found there, and sparse transitions and the start and end assertions
  are charged at their real cost. A 64-character hex, a bounded email, a bounded Unicode name and a bounded `.`
  pattern now decide inputs up to the JSON limit at defaults, base64 up to 2.6 MB (was 725 KB). The hostile patterns
  are refused as before, and the longest search the default ceiling admits still takes about 120 ms natively. The
  limit's name, default and code are unchanged. `PATTERN-WORK.md` in the evaluator crate derives the bound.
- `explain` returns the verdict `evaluate` returns at every limit setting. At defaults the faster lane could
  establish a failure that the explaining lane then could not, because only the explaining lane has the pattern
  ceiling; an explanation now keeps the established failure and reports its problems as incomplete when it cannot
  finish them.
- TypeScript `initialize()` stays retryable when the caller's input or the host's loading rejects with a
  `WebAssembly.RuntimeError`. Only a trap raised while SDK Wasm runs marks the instance unusable.
- `KindError::LimitExceeded` no longer prints its label in `Display` or `Debug`, and `RequestError::Shape`,
  `PullValidationError::LimitExceeded` and `ComparisonFailureKind::LimitExceeded` no longer print theirs in `Debug`.
  Each label stays reachable by matching.
- `PullValidationError::source()` returns the `RelationshipError` for a relationship refusal.
- The evaluator test-support runner compares explanations exactly, so two pointers that differ only in unpaired UTF-16
  units are no longer judged equal.
- TypeScript reports a malformed or inherited options object with the option's own `SdkError` code (for example
  `invalid-preparation-limits`) where some routes threw `TypeError` `invalid-json-object`. What is accepted is
  unchanged.
- A TypeScript registration refusal for a limit reads `external schema registration exceeded a preparation limit`,
  without naming a Rust type. The code stays `resource`.

### Added

- The API-reference check denies every rustdoc warning, not only broken links; a malformed tag had been rendering a
  source-pull page wrongly on docs.rs.
- A regression that a provider's `ERR_REFUSED` reason, an explicit `null` reason and no reason each reach the caller
  unchanged through checked binding and operation invocation and preflight.

## 2026-11-14 owner rulings (unreleased)

### Changed

- Breaking: the kind-description crate's `CarrierError` is a struct with `kind()` returning a `Copy`
  `CarrierErrorKind` and `pointer()` returning the failing location when known, the shape the source-pull crate's
  carrier error already has. Neither crate's carrier category prints a label in `Display` or `Debug`, including one a
  caller constructs; read it through `kind()`.
- Breaking: the source-pull crate's `RelationshipError` is a struct with `kind()` returning a `Copy`,
  `#[non_exhaustive]` `RelationshipErrorKind` (the twelve former variants) and `pointer()` returning the offending
  row of the provider's report. Where two rows contradict each other the later one is named; missing coverage has no
  row. `Display` names no location.
- A single uncancellable pattern search on the evaluator's own lane may be charged at most 250,000,000 work units by
  default; a larger search returns `no-verdict` with `pattern-search-work-limit` before it runs. Rust
  `Limits::pattern_search_work_units` and TypeScript `EvaluatorLimits.patternSearchWorkUnits` raise it or remove it
  (`None` / `null`). The two hostile patterns from the review, which ran 6 to 21 seconds and could not be cancelled,
  are now refused in under 2 ms.
- TypeScript turns a Wasm runtime trap (memory exhausted, for one) into an `SdkError` with code `wasm-trap`, and marks
  the instance unusable: every later SDK call fails at once with the same code, where it used to half work.
  Disposal after a trap is safe and finalizers never throw.
- TypeScript `locate` returns `undefined`, not `null`, for an unavailable location, as every other optional read does.
- TypeScript ordinary values refused at an admission limit report `byte-limit`, `node-limit` or `depth-limit`, the
  codes exact text reports for the same causes, where they reported `input-limit`.
- Breaking: TypeScript entry reads (`operations()`, `operation(key)`, `resolveOperation(name)`, `bindings()`,
  `binding(key)`, `sources()`, `source(key)`, `dependencies()`, `dependency(key)`, and examples) return detached,
  frozen records of the entry's fields, with nothing to dispose. What needed an entry is a query on the proof by
  primary key: `inputSchema`, `outputSchema`, `operationBindings`, `examples`, `example`, `sourceData`,
  `bindingData`, `exampleInput`, `exampleOutput` and the `operationValue`-style queries. `acceptsKind` is a plain
  function. Schemas, exact JSON, documents, proofs, resource sets and prepared schemas stay owners, and a forgotten
  schema or exact value still keeps its whole document in Wasm memory until it is disposed or finalized.
- The TypeScript package's generated bindings now include a nested module under `dist/wasm/snippets/`, which
  the entry-record transport needs. A bundler or host that copies `dist/wasm` must keep that layout.
  `build-info.json` records every runtime module by relative path and SHA-256, and the browser, workerd,
  packaging and reproducibility tools take their file lists from it.
- Preparation charges its allowance for the storage it actually holds, at the time it holds it, where it charged
  conservative estimates many times larger. The smallest allowance that prepares 3,000 records falls from 105 MiB
  to 6.3 MiB and 1,000 records from 36 MiB to 2.1 MiB; on its own workloads the charge is 1.0 to 1.7 times a
  counting allocator's held bytes. Defaults and lane selection are unchanged.
- Breaking: schema comparison follows the amended schema-comparison profile 0.1 (interfaces `eafa49a`).
  Compatible is a proof of inclusion; where the profile cannot prove it, the answer is indeterminate. Incompatible
  outranks indeterminate in every summary, and no property name or rule order decides a verdict. Objects compare
  every member's effective schema in both directions; a non-identical `oneOf` position stops as indeterminate; rules
  apply only to the types that can flow; finite values, unions, integer endpoints and empty length domains withhold
  failures that are not counterexamples. `Comparison::finding()` tells an unresolved obligation from an
  outside-profile boundary. Four of the 95 earlier corpus groups change summary; all 241 pass.
- Breaking: normalization maps Boolean schemas at every schema position, strips annotations, `x-` members and
  `$comment` inside retained content, canonicalizes `type` and `required` there, unwraps a single-branch `allOf`, and
  keeps each value of an `enum` intersection once. A schema reached through an external URI keeps the dialect JSON
  Schema assigns it.
- The interfaces pin moves to `eafa49a`: an application keys a kept context value by the exact source entry, the
  source-puller reference rule governs reuse as well as existing entries, and `ERR_REFUSED` may carry a reason whose
  shape the source's kind defines. The guides say so.

### Added

- `SchemaValidator::retained_bytes()`, defaulting to `None`, with `PreparedSchema::retained_bytes()` and
  `PreparedOperation::retained_bytes()` passing it through. The default evaluator answers on its own lane, within 3
  percent of a counting allocator on the measured shapes, and `None` on the lane that runs the `jsonschema` library,
  whose allocations it cannot see. Nothing is computed until asked.

### Fixed

- The evaluator's work counter was pointer-width, so on wasm32 a pattern charge above 2^32 was reported as a work
  limit nobody set and Rust and TypeScript answered one input differently. It is 64-bit, and accumulates only when a
  cap is set.

## 2026-11-14 panel shapes (unreleased)

### Changed

- Breaking: `SchemaOrigin::source_uri` is now `retrieval_uri`, with its constructor parameter and serialized
  property; TypeScript uses `retrievalUri`. The field names the address a supplied schema document was retrieved
  under. "Source" is the specification's word for something else. `ReferenceRefusal::resource_uri` is a different
  field and keeps its name. The largest fixed-schema diagnostic grows by the three bytes of the longer name.
- TypeScript `ExactJson.metadata.kind` is typed as the exported six-member `JsonKind` union, where it was `string`.
- `Invocation` is `#[must_use]`: discarding this lazy owner warns that provider and validation work did not run. A
  caller abandoning an attempt on purpose drops it explicitly. No other handle gained the annotation.

### Added

- A Workers guide (`docs/workers.md`): which limits to set and the rule behind each value, what preparation
  allowances admit, handler habits, and what a caller sees when Wasm memory runs out. Every table is regenerated by
  `node tools/workers-memory/run.mjs` against a built package. The performance notes and support table no longer say
  the defaults fit a typical container or that document memory grows with bytes alone, and two figures for
  contained against supplied preparation that no workload in the repository reproduces are withdrawn.
- TypeScript can set preparation's allowances: `preparationLimits` on `PrepareOptions`, registration options on
  `ExternalSchemas` construction and `with`, and `preparationLimits()` for the defaults, which are unchanged. A
  limit set here refuses with the same code as the same limit set in Rust. This is how a host with a small
  memory ceiling, such as a Worker, turns a schema too large for it into a refusal with a code: at the default
  allowance of 512 MiB a module that runs out of memory traps instead.
- Ordinary JavaScript encoding failures keep their `TypeError` or `RangeError` class and message and gain a stable
  `code`. Ordinary admission limits reported `input-limit` here (the owner rulings above replace it with the
  exact-text codes); exact-text admission keeps `byte-limit`, `node-limit`
  and `depth-limit`.
- `Pointer::to_exact_json()` exports a complete JSON string token that keeps every UTF-16 unit, so a pointer into
  a draft whose member name holds an unpaired unit selects the same member after transport. Ordinary Serde output
  is unchanged and still replaces such a unit with U+FFFD.
- `ResourceIdentity` implements `Hash`, consistent with its equality: the same environment allocation and the same
  resource within it. Equal URI text in two environments gives two keys.
- `ComparisonContext::retained_bytes()` estimates, when asked, the source arenas, configuration and completed indexes
  the context keeps alive. It is per owner: sums across owners that share storage overcount. It is neither resident
  memory nor what dropping one handle frees.

## 2026-11-14 review repairs (unreleased)

### Changed

- A comparison context whose root is an OpenBindings document ignores a caller-set
  base. The document's own resource cannot be named by any reference
  (specification section 7.2), so a reference to it through a retrieval address is
  a missing reference, as it is without the base. A base on a standalone schema
  root keeps its meaning.
- Comparison follows a reference only to a schema position of its resource. A
  target that is instance data (under `const`, `enum`, `default`, `examples`, or
  an unknown keyword) fails as `MissingReference` and is no longer compared as a
  schema. A standalone schema that kept definitions under a member such as
  `schemas` or `components` must keep them at a schema position such as `$defs`.

### Added

- The kind guide covers what eight independent adapter authors had to find in
  the source: a worked source checker, reading exact JSON, placing schemas taken
  from a protocol artifact, what an adapter caches, depth bounds for a recursive
  decoder, matching existing entries by a stable identity, cleanup that outlives a
  cancelled call, and the two shapes of a transport-generic adapter with what each
  does to `Send`.
- Compiled examples back each of those, and one shows a host that holds two kinds:
  object-safe twins of the role traits, an error type that keeps the adapter's own
  error behind `source()`, a registration check against published support, and a
  context store keyed by the exact source entry.


- Tests that fail if comparison cost grows faster than n log n for enum, const and
  required membership, measured in the SDK's own work units, and tests that a
  cancelled preparation, evaluation or comparison returns within a fixed number of
  further polls whatever the input's size.
- The TypeScript memory test now asserts the two Wasm memory ceilings the
  performance notes say it guards (224 MiB and 228 MiB).


- The kind guide now states the pull report's label forms and links the interface's Report section, says that a
  pull offering schemas needs the comparison companion and which root its references resolve in, links the challenge
  conventions, and adds the rule against hash tables keyed by input content to its checklist. Compiled examples cover
  a report with current and stale schema entries and a conflict taken through the checked pull, and one adapter whose
  invocation future is `Send` on native targets and local on wasm32.

### Fixed

- Directional enum, const and required membership uses an ordered lookup in place
  of a linear search for each value: 32,000 strings against 64,000 fell from 13.4 s
  to about 36 ms. Verdicts and reasons are unchanged. A union walk stops at its
  first incompatible variant.
- The documented count of ordinary directional fixture groups is 65.

- `JsonValue::parse_prefix` reports an invalid string escape or a malformed literal
  as `Invalid` at once, at the defective byte. Before, an invalid escape inside an
  unfinished string was reported as `Incomplete` until the closing quote arrived.
- The guidance for `parse_prefix` now says what the retry rule assumes. Waiting
  for the buffer to double keeps work linear for a file or a batch and stalls a
  peer that sends one message and waits for the reply; retrying on every read
  reparses from the start each time. A protocol with its own delimiter should find
  the delimiter and parse once.

- The recorded Wasm asset size and the development loop's opening paragraph are
  current.

- The evaluator's own lane checked a value against an enum by walking every choice, and kept walking after a match,
  so an array of n values against n choices cost n squared comparisons. Within one evaluation, after `ilog2(m)`
  lookups against the same enum of m choices (more than eight), it now sorts the choices once under an order whose
  equal class is the evaluator's literal equality and answers the rest by binary search; every walk stops at its
  first match. Preparation does no new work, a single value and a small enum cost what they did, nothing is kept
  between evaluations, and verdicts and failing keywords are unchanged. Work for 1,024 values against 1,024 choices
  falls from 70,947,865 units to 2,790,629.

- Comparison resolved a reference to an embedded or supplied resource only under the exact spelling of its URI,
  where the evaluator also accepts the same URI with a different case of scheme or host, a percent-encoded
  unreserved character, a different case of percent-encoding, or the default port. It answered `MissingReference`
  for such a reference, so one SDK read one reference two ways. Comparison now uses the core's own URI
  canonicalization for a resource's `$id`, a supplied resource's URI, a caller's base and a reference, and a test
  asks both for every spelling in a table of 23 and requires them to agree. Supplied resources registered under
  two spellings of one URI are refused as the evaluator refuses them.

- `uniqueItems` compared every pair of items that shared a fingerprint, so a group of g distinct colliding items
  cost g squared comparisons, and on wasm32, where the fingerprint seed is not secret, a sender who knows the build
  can make items collide. A group of more than eight is now sorted under the evaluator's exact order and checked
  neighbour to neighbour. Arrays without collisions cost what they did; verdicts are unchanged.

## 2026-11-13 panel decisions (unreleased)

### Changed

- Breaking: `check_interface` and `check_interface_with_control` now take the two
  conformance proofs followed by their two caller-configured comparison contexts.
  Contexts must retain their paired proof's exact root; `ContextMismatch` identifies
  a mismatch. One shared attempt still covers the entire fold.
- Breaking: `ComparisonFailure` is a Clone struct with a Copy
  `ComparisonFailureKind` returned by `kind()`, optional original schema location
  from `origin()`, and established operand from `side()`. Match the kind instead
  of error variants; whole-error equality includes provenance. Automatic
  formatting remains redacted and retained locations stay within the allowance.
- Breaking: checked `pull_source` and `pull_source_with_limits` now return
  `(PullResult, ConformantDocument)`, preserving the already proved snapshot with
  all additions and no stale proposals applied.
- Breaking: source-pull `CarrierError` is now a struct with a Copy
  `CarrierErrorKind` from `kind()` and an optional report-row `pointer()`. It
  retains Clone, PartialEq and Eq but no longer implements Copy. Nested result
  errors prefix known report rows with `/report`.
- Breaking: `RequestError`, `PullValidationError`, `PullFailure` and `PullError`
  are now non-exhaustive. Downstream matches must handle unknown failures.
- Breaking: reference inspection returns `ReferenceReport::Complete { references }`
  or `Incomplete { retained, limitation }`. TypeScript discriminates on `complete`
  and exposes `references` only in the complete branch. Serialization follows
  these alternatives. Validation findings, value problems and pull reports keep
  their fields.

### Added

- Consuming `map_error` methods on `CheckFailure`, `KindError`, `PullFailure` and
  `PullError` convert their generic error parameter while preserving every other
  variant and payload, without adding bounds on the error or allocating.
- `JsonValue::parse_prefix` and `parse_prefix_with_limits` return the first exact
  value in a byte buffer and its exclusive token-end offset, with a distinct
  `JsonPrefixError::Incomplete`. Calls inspect only the value and constant
  lookahead. Hosts own separators and I/O, grow incomplete buffers geometrically
  before retrying, and use `parse_with_limits` on incomplete input at final EOF.
  Admission limits apply through the value end. Invalid UTF-8 in later buffered
  bytes is deferred.
- `ReferenceLimits { max_report_bytes }`, `references_with_limits` and
  `references_with_resources_and_limits`; TypeScript reference options accept
  `limits.maxReportBytes` and `referenceLimits()` exposes the defaults. The default
  retained-text allowance remains 8 MiB.
- `ValidationLimits { max_findings, max_pointer_bytes }` and
  `Document::validate_with_limits`; TypeScript offers `validate({ limits })` with
  `ValidationOptions` and `validationLimits()`. Defaults remain 4,096 findings and
  8 MiB of generated
  source-pointer text. Retention never changes rule evidence or the conclusion.

## 2026-11-13 experiment repairs (unreleased)

### Changed

- Under default limits, a resource set that retains more than 512 MiB of parsed
  external schema roots is refused at registration. `ExternalSchemas` now charges
  each distinct root it retains, once; clones share the charge.
- The interfaces pin moves to `a3c19658fc89815d7a417eae29ba31fd4e930daf`, which
  states the comparison profile's union precedence and adds three fixtures.

### Fixed

- Resolving a schema identifier no longer leaves it in a process-wide cache.
  A long-lived host that prepared and dropped many resource sets kept every
  identifier it had ever resolved.
- TypeScript: a non-string argument throws `TypeError` instead of trapping in
  Wasm, an oversized resource URI crosses the boundary only as its refusing
  prefix, and raw handles are taken after caller code has run.
- `ResolutionFailure` prints its category, and `ReferenceTarget`'s `Debug` no
  longer prints an anchor name.
- Preparation folds single-use shallow leaf definitions into the compiled
  bundle, which lowers peak memory on definition-heavy schemas. No result
  changes.
- A union comparison no longer depends on the order its variants were written
  in. A variant that nothing covers decides incompatible before an undecided
  variant decides indeterminate; before, whichever came first decided.

## Limits principle (unreleased)

### Changed

- Breaking: `ComparisonLimits::max_work` is now `Option<usize>`, `None` by default.
- Breaking: `PreparationLimits::max_sdk_work_units` is now `Option<usize>`, `None` by default.
- Breaking: `PullLimits::max_relationship_work` is now `Option<usize>`, `None` by default.
- Time bounds belong to the caller, through cooperative cancellation or an explicit
  work allowance. Preparation documentation includes a host-specific suggestion
  for about 250 ms; work units are not portable durations or hard deadlines.

### Fixed

- Comparison, preparation and pull relationship memory allowances release
  completed scratch, keeping their existing default sizes. Retained reports,
  indexes and prepared programs stay charged; conservative mixed construction
  estimates remain charged with their owner.

## 2026-11-12 review follow-ups (unreleased)

### Changed

- Breaking: interface role callbacks now receive the required operation and each
  matched provided operation, or `None` for a missing match.
- Breaking: `OperationComparison::role()` now returns `Option<OperationRole>`;
  missing operations have one correspondence with no role.

### Fixed

- Comparison reference indexing is independent of selection history and resource
  occurrence sharing, and contexts release detached inputs after each call.
- Reference-rule leaves match schema positions through shared targets; retained
  `$defs` no longer marks its parent outside the comparison profile.
- Interface cancellation returns `Cancelled`, including during the final slot.
- Normalized reference targets are reused within comparison calls.
- TypeScript resource staging stops at the first entry exceeding the default
  count or cumulative URI byte bound, preserving Rust registration errors.

## 2026-11-11 panel decisions (unreleased)

### Changed

- Breaking: `Context::new` now returns `Result<Context, ContextShapeError>`, and
  `TryFrom<JsonValue> for Context` returns `ContextShapeError` instead of
  `JsonAccessError`. Context refuses an object anywhere in it, including inside
  arrays, that repeats a decoded member name. This is SDK admission policy (the
  context document is silent on duplicates) following OBI-01's parser-differential
  reason.
- Every request decoder that admits context applies the same check through its
  own error: `BindingRequest` and `OperationRequest` return
  `BoundaryFailureKind::DuplicateMember` at `/context`, `PullRequest` returns
  `RequestError::Shape("duplicate-member")` and `CheckSourceRequest` returns
  `CarrierError::DuplicateMember`. Diagnostics stay value-free.

### Added

- `ContextShapeError` with `ExpectedObject` and `DuplicateMember`.
- The adapter guide states when `can_invoke` and a descriptor's `supports` must
  agree, with a checked public-API test in the kind adapter example.
- `edit` and `carriers` fuzz targets with committed seeds and stable replay.

## 2026-11-11 review follow-ups (unreleased)

- Lend prepared input and output schemas from `PreparedOperation` for checks
  before loading context or opening an invocation.
- Construct kind-description and source-check requests from typed values.
- Bound supplied-root admission and lookup before normalization or copying,
  pass evaluators the stored normalized identity, and reuse the set's resolution
  space while checking each preparation's limits.
- Qualify evaluators against supplied roots, including local definitions,
  recursion, relative references and unavailable retrieval identities.
- Clarify that selection does not perform request address admission.

## 2026-11-11 comparison rework (unreleased)

- Repin every applied interface to 484c9399207de99bd56921cead5f44a7b5740d9e and vendor the amended corpus byte-identically.
- Apply rule 0a and normalization steps 3 and 9: both union keywords fail closed, retained references compare targets, and `$defs` retention is schema-local.
- Add reusable lazy `ComparisonContext` inputs, shared indexing and default-limit scale coverage.
- Check every alias correspondence with caller-selected operation roles; keep failures for caller summary policy.
- Correct reference leaf accounting by schema position, expose resolved targets and reuse contexts in the source-pull example.
- Add comparison fuzzing and refresh comparison guides and API reference inputs.

## 2026-11-11 directional comparison (unreleased)

- Implement profile OB-2020-12 0.1 directional input/output verdicts with exact reasons and bounded work.
- Add interface checking with explicit required/provided roles, sorted per-operation outcomes, missing matches and profile verdict collapse.
- Add source-puller reference-rule identity and use it for existing entries in the larger pull example.
- Qualify all 59 ordinary and 29 exact directional fixture groups alongside the existing identity corpus.

## 2026-11-11 supplied schema roots (unreleased)

### Changed

- Breaking for evaluator implementers: `EvaluationRequest::schema` is replaced
  by `root: SchemaRoot`, selecting `Contained(&Schema)` or `Supplied(&str)`.
  `EvaluationRequest::environment` enters either original context.
- `ExternalSchemas::prepare` and `prepare_with_control` prepare a supplied
  resource at its retrieval URI as its own JSON Schema root. TypeScript exposes
  `ExternalSchemas.prepare(uri, { limits, signal })` through Wasm. Local
  references, declared IDs, dialects and retrieval provenance stay with that
  resource; missing root URIs are preparation failures.
- Kind data and context challenge guidance uses supplied roots. Core `Schema`
  still comes only from a conformant OpenBindings document.

## 2026-11-11 binding selection (unreleased)

- Add `DefaultOperationInvoker::select` and `PreparedOperation::select`, returning
  the core `Binding` before context loading or provider work. Invocation,
  preflight and selection share binding resolution rules. Selection does not
  admit the address; malformed ordered lists return `ERR_BINDING_NOT_FOUND`
  there and a `BoundaryFailure` from invocation or preflight.

## 2026-11-11 review fixes (unreleased)

- Cancel pending binding and operation preflight work before provider cleanup.
- Preserve source-pull and publication error causes and give source-pull wrappers fixed layer messages.
- Correct documentation for admission limits, execution disclosures, locations, discovery diagnostics, drift and invocation code strings.
- Walk TypeScript arrays one element at a time and count object members before reading their
  descriptors, so an over-limit ordinary value is refused at the same node without first expanding it.
- Validate reference-inspection options and their resource and cancellation types.
- Preserve lifecycle errors during exact-to-ordinary JSON conversion.
- Send stable JSON error-kind names across the Wasm bridge.
- Accept empty Content-Encoding list members in native discovery.
- Merge agreeing proposal writes in the source-pull update recipe and reject conflicting writes.
- Reuse source-pull example operations through keys and aliases, reporting ambiguous matches as conflicts.

## 2026-11-11 compiled-lane mutation follow-up (unreleased)

### Changed

- A compiled-lane `pattern-total-memory-limit` refusal no longer composes a
  message that never reached callers; the refusal itself is unchanged.
- Compiled qualification adds a test that recursive schemas keep the reservation
  charge for their scalar `const` and `enum` literals.

## 2026-11-10 decision rulings (unreleased)

### Changed

- Apply interfaces `484c9399207de99bd56921cead5f44a7b5740d9e`, which states the
  kind-descriptor `check` rule; retain specification
  `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`. Every copied interface fixture is
  byte-identical at the new revision.
- SDK diagnostic `Display` and `Debug` use fixed SDK text and constants. Values
  remain available through explicit fields, accessors and serialization. Wrapper
  messages print their own layer and expose causes through `Error::source()`.
- `SourceLocation` displays line, column and byte only; `pointer()` borrows its
  retained pointer. `VersionRefusal::declared()` borrows its retained declaration.
- `OperationPreparationFailure::source()` exposes resolution and schema
  preparation causes; cancellation has no source.
- Whole-document validation, HTTP discovery, publication and TypeScript share
  `JsonLimits::default()`, including 10,000 container levels. Beyond the cap,
  validation is undetermined with `depth-limit`; explicit limits admit deeper input.
- Kind descriptions require `check` exactly when `supports` names
  `openbindings.kind-descriptor.checkSource`, matching `pull` and `invoke`.
- Compiled qualification keeps 15 of its 48 private tests: the four release
  stack gates, the independent default-limit differential, and ten tests that
  each catch a compiled-lane defect no other evaluator test catches. The other
  33 caught nothing the remaining tests miss. Kept tests use behavior names and
  write no receipts; their only clock is a deadline that fails an evaluation that
  does not terminate.

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
