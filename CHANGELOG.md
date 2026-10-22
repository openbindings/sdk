# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- The npm package ships compiled JavaScript and declarations without source maps; each build
  starts from an empty `dist`.
- Every package stays unpublished until an authorized release: Cargo `publish = false` and npm
  `"private": true`, checked by `tools/verify-publication-guards.py`.
- Value contracts come from the operation. A conformant operation's `input()` and
  `output()` return a `ValueContract`; `prepare(&evaluator)`, `prepare_with(&evaluator,
  &resources)` and `prepare_with_control` borrow the evaluator and return
  `Result<PreparedValueContract, NoVerdict>`. `PreparedValueContract::validate_json` checks exact
  JSON text. TypeScript mirrors this with `ConformantOperationView.input()`/`output()`
  and `ValueContract.prepare(options)`, whose ready result carries its
  `PreparedValueContract` as `valueContract`.

- One exact OpenBindings 0.2 `Document` for parsing, inspection and in-place
  editing: atomic batches of JSON Pointer edits and typed part maps, and typed
  field and part setters, all keeping untouched bytes exact. Every edit refuses
  with one `EditError`; a typed part's field collision carries a pointer into
  that part (TypeScript `EditError.partPointer`), and a typed part that cannot be
  encoded as supported exact JSON refuses with `EditError::Serialization`
  (`serialization`). Validation with original source locations, bounded
  findings and per-rule evidence.
- Document validation's memory grows linearly with the document. Compared
  identifiers share runs of the `$id` texts, so long and nested `$id`s take
  memory and time linear in the document, as do repeated findings deep in a
  document, large maps, alias lists and `uniqueItems` arrays, and fixed checks
  read names holding U+E000 in place. Of the diagnostic memory probe's 32
  documents at the default limits (64 MiB, 1,000,000 nodes, any nesting), the
  largest allocates 281.5 MiB beyond the caller's input, counting each
  reallocation as holding its old and new blocks at once.
- Document validation's fixed checks (OBI-02, OBI-10) and reference inspection use
  the unmodified jsonschema 0.58.6 library with its default pattern engine, which
  matches every fixed pattern on a finite automaton, exactly as ECMAScript does.
  The checks have no work budget; their work is linear in the admitted document,
  and a 60 MB SemVer near miss reaches its verdict in about 0.6 seconds. The SDK
  carries no fork of any dependency.
- `Conformant<Document>` (TypeScript `ConformantDocument`), the proven state, with
  infallible views, operation and alias lookup, exact schema access, reference
  inspection and opaque kind comparison. It is the only route to value contracts.
- Explicit immutable resources and reusable input/output value contracts, with
  preparation refusals separate from satisfies, fails, undefined or no-verdict outcomes.
  Undefined results carry a code, message and optional original schema location;
  no-verdict describes inability to answer.
- An optional JSON Schema 2020-12 evaluator composed over the unmodified
  jsonschema 0.58.6 library, with exact numeric semantics, ECMAScript `u`-mode
  patterns on a linear-time engine, dynamic references resolved through each
  path's dynamic scope, and cooperative cancellation. It bundles each contract
  into one resource, so every reached schema compiles once, and reuses a
  resource set's identity index instead of indexing its resources again. A
  contract that reaches a resource that was not supplied prepares with an empty
  stand-in for it, and a pass that reaches the stand-in gives no verdict
  (`resource-unavailable`) naming it. A contract in which an
  `unevaluatedProperties` or `unevaluatedItems` keyword's annotation sources
  reach a stand-in, a reference to a fragment of a missing resource, or one that
  identifies no schema refuses preparation. Patterns, including the word
  boundary `\b`, are translated for a linear-time engine and match in time
  linear in the string, so there is no pattern work cap; one that needs a
  backtracking engine (lookaround or backreferences), applies a Unicode property
  escape or holds the non-word boundary `\B` gives no verdict. The evaluator
  bounds what the SDK itself reads, writes and recurses through, and does not
  estimate the library's compiled memory. Limits that protect the process have
  defaults, and those that encode an assumption about the host are expressed as
  the host's resources: `Limits::stack_bytes` (TypeScript `stackBytes`,
  256 KiB) is the stack preparation and evaluation may use, from which the
  evaluator sizes evaluation depth (256 bytes for each nested schema
  evaluation), bounded from the contract before a value is evaluated, the stack
  a pass of a contract holding an unevaluated keyword may use as it evaluates
  (half, measured as it goes) and the unevaluated walk limit below, so a host
  that gives a larger thread admits deeper values and longer walks; each
  pattern's size, form
  and nesting, and the forms kept for all of a contract's patterns together,
  counted as each is written, with each distinct `pattern` form kept once
  (`Limits::total_pattern_bytes`, TypeScript `totalPatternBytes`, 4 MiB,
  `pattern-total-byte-limit`); the regex crate's memory for all of a
  contract's compiled patterns together (`Limits::total_pattern_memory`,
  TypeScript `totalPatternMemory`, 64 MiB, `pattern-total-memory-limit`, whose
  message states the arithmetic) for each distinct `pattern` form the run
  matcher does not match and each `patternProperties` name: a quarter, at most
  2 MiB each, for their search caches (`dfa_size_limit`) in equal parts, and
  the rest for their compiled forms (`size_limit`), each first within 48 KiB,
  with the forms that need more sharing what is left, at most 32 MiB each, so
  small patterns elsewhere change a large one's outcome only through their
  48 KiB each, no outcome depends on the order strings reach the patterns, a
  contract compiling more than 1,024 patterns refuses before compiling any, 40
  counted-repetition names that peaked at 1.1 GB refuse in about 0.2 s, and a
  `pattern` value compiled when a string first reaches it stays within what it
  is given; the memory dynamic specialization adds to a contract's program,
  estimated for the SDK's own copies and dynamic contexts
  (`Limits::program_bytes`, TypeScript `programBytes`, 48 MiB,
  `program-specialization-limit`); and an unevaluated keyword's annotation
  walk, which the library compiles by recursion, weighed by the kind of each
  reference against what `stack_bytes` admits (a fifth left to the caller,
  99 KiB to the rest of preparation, 1 KiB per unit), so the longest admitted
  walk of every kind prepares with a fifth of the thread already in use on
  Linux, macOS and Windows from 192 KiB to 2 MiB (`unevaluated-walk-limit`: at
  the default 26 `$ref`s, 8 levels of `allOf` inheritance, 7 through `then`,
  `else` or `dependentSchemas`). Each of these refusals states its arithmetic
  and names the setting to raise.
  Preparing a schema from an untrusted party can still take more memory than a
  host may want, so a host doing so should prepare in an isolated worker or
  process with an operating-system memory limit. Limits that would only bound
  time are the application's to set and are off by default:
  `Limits::evaluation_steps` caps each pass's work, without which time can grow
  exponentially with some schemas, and `Limits::schema_resources` (TypeScript
  `schemaResources`) caps the schema resources one contract may reach, refusing
  with `LimitExceeded` and `schema-resource-limit` before compiling. The default
  evaluator never returns `Undefined`. One schema transform keeps the library's
  `unevaluatedProperties` walk clear of an upstream bug that drops compiled
  pattern subschemas.
- Cheap checks and explanations are chosen per call. `PreparedValueContract::validate`
  (and `validate_with_control`, `validate_json`) reports a failure without
  problems; `explain` and `explain_with_control` return the same verdict with the
  library's first error at its keyword's original location, with a fixed
  message. TypeScript `PreparedValueContract` gains `explain` beside `validate`.
  A prepared value contract keeps only the original locations of the schemas it
  compiled, never their text, its document, supplied resources or a location map.
- `operation::Invoker::with_validation_detail(Detail::Explain)` asks the invoker to
  explain a rejected value before withholding it, so `ValidationFailure::outcome()`
  carries the evaluator's problems; by default it carries the verdict only.
- An evaluator integration trait, whose `PreparedSchema::validate` receives the
  call's `Detail` (`Outcome` or `Explain`), and a reusable qualification suite
  that checks and explains every value, including explanations interrupted by
  cancellation.
- HTTP discovery and publication with bounded bodies and explicit host-policy
  responsibilities. The decoded body limit protects the process and defaults to
  1 MiB. An attempt has no deadline unless the application sets one, since a
  deadline only bounds time: Rust `ClientOptions::timeout` takes
  `Some(duration)` and TypeScript takes `timeoutMs`, and a zero duration is
  refused with `invalid-timeout`. Publication serves the well-known path
  when a request spells an unreserved character percent-encoded, as in
  `/.well-known/%6Fpenbindings` (RFC 3986 section 2.3), and never decodes a
  reserved escape such as `%2F` into a path separator.
- Provisional Rust provider traits and checked callers for source inspection,
  interface synthesis, binding invocation and operation invocation. The three
  kind-facing companions share one `SupportedKind` and `KindSupportVerdict`, so a
  provider serving several jobs builds one advisory list. Inspection and
  invocation share one by-value `BindingValue`: an inspected target's binding
  invokes as it is, and `bind(operation, source)` gives the author's core binding;
  its `BoundaryError` converts into an `InspectionError` that keeps the boundary
  code and location. A synthesis provider's typed-part edit over the byte budget
  now refuses as `openbindings.synthesis.byte-limit` instead of
  `openbindings.synthesis.authoring`.
- A TypeScript API with bundled Wasm, exact-value editing, disposable owners and
  Fetch discovery for qualified browser, Node ESM and Worker hosts. Its names
  follow Rust's where the concept is the same: `Document.validate()` returns a
  `DocumentValidation`, whose conformant branch carries the proof, and
  `validateDocument` returns the plain `DocumentReport`. `DocumentValidation` thus
  changes meaning: it now names the validation carrying the proof (formerly
  `ValidationResult`), while the plain report it named before is `DocumentReport`.
- A bundled Wasm module whose bytes do not depend on where it was built. The
  build gives the builder's Cargo home, toolchain, target directory and checkout
  fixed names in the module, and refuses a module that still embeds one; CI
  rebuilds it from another checkout path and Cargo home and requires identical
  bytes.
- Runnable first-use and lifecycle examples, legacy Go and TypeScript migration
  guidance, API reference documentation and reproducible performance measurements.

This first release remains unpublished. Package and specification versions are
independent.
