# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- The npm package ships compiled JavaScript and declarations without source maps; each build
  starts from an empty `dist`.
- Every package stays unpublished until an authorized release: Cargo `publish = false` and npm
  `"private": true`, checked by `tools/verify-publication-guards.py`.
- The release candidate's `publicationOrder` puts each crate after every local crate it
  depends on, development dependencies included, so Cargo can package and publish the
  crates in that order; the packager refuses an order that breaks the rule.
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
- The default document byte limit is 32 MiB (33,554,432 bytes), down from 64 MiB,
  because validation memory grows with the document and the protective default
  should fit a typical container, and the admission limits are a setting at the
  calls that admit documents and values: `JsonLimits` for
  `validate_document_with_limits`, `Document::parse_with_limits` and
  `JsonValue::parse_with_limits`, and `limits: { maxBytes, maxNodes, maxDepth }`
  for TypeScript's `validateDocument`, `Document.parse` and `parseJson`, whose
  unspecified entries keep the `JsonLimits` defaults (malformed limits throw
  `invalid-json-limits`). `Document::new`, `Document::from_json`,
  `JsonValue::parse`, `JsonValue::string`, `Publication::from_bytes` and
  `validate_json` use the defaults, as do TypeScript's `Document.create()`,
  `ExactJson.from`, `validateJson` and ordinary-value `validate` and `explain`.
  A document keeps the limits it was admitted
  under (`Document::limits`, TypeScript `limits`), and its edits and typed
  setters apply them to the edited document, the same way in both languages.
  Value-contract preparation keeps its own fixed bounds, as before. Discovery
  validates a body with its byte limit raised to the configured body bound and
  the default 1,000,000 nodes. Every admission and edit limit refusal names how
  to admit more in both languages, including the route for calls that take no
  limits; the projection's fixed bounds have no setting.
- Document validation's memory grows linearly with the document. Compared
  identifiers share runs of the `$id` texts, so long and nested `$id`s take
  memory and time linear in the document, as do repeated findings deep in a
  document, large maps, alias lists and `uniqueItems` arrays, and fixed checks
  read names holding U+E000 in place. Of the diagnostic memory probe's 34
  documents at the default limits (32 MiB, 1,000,000 nodes, any nesting), the
  largest allocates 256.0 MiB beyond the caller's input, counting each
  reallocation as holding its old and new blocks at once.
- The text compared identifiers copy out of `$id`s whose dot segments separate
  their segments is kept in blocks that never move, instead of one buffer that
  doubled: each new block is as large as the text before it, but no larger
  than the identifiers still to come can copy. Text once kept is never copied
  again, and growing it leaves no old buffer behind, which WebAssembly's
  linear memory, unable to release it, kept resident. The probe's largest
  document fell from 272.2 to 256.0 MiB (from 412.2 to 364.9 MiB at a 64 MiB
  limit); through the TypeScript package its validation adds 278.5 MiB to
  Node's resident memory, from 317.0 MiB.
- TypeScript's `validateDocument`, `Document.parse`, `parseJson` and
  `validateJson` encode text input straight into module memory instead of
  first into a JavaScript copy of its UTF-8, so text crosses into the module
  once, as bytes do. Results are unchanged.
- TypeScript checks text for unpaired UTF-16 with the host's
  `String.prototype.isWellFormed`, which Node 22, Chromium, WebKit and workerd
  provide, and scans the text itself only on a host without it; both refuse
  the same texts. Checking 32 MiB of text no longer takes a 65 ms scan.
- Owned input: `validate_document_owned(Vec<u8>)`,
  `validate_document_owned_with_limits(Vec<u8>, JsonLimits)`,
  `Document::parse_owned(Vec<u8>)`, `Document::parse_owned_with_limits(Vec<u8>,
  JsonLimits)` and `JsonValue::parse_owned_with_limits(Vec<u8>, JsonLimits)`
  take the caller's buffer as the document's or value's text instead of
  copying it, with exactly the results, reports, errors, messages and
  locations of the borrowed forms. The SDK hands over the buffers it builds
  itself, for document edits, ordinary-value conversion, synthesized
  interfaces, invocation carriers and inspection models, and TypeScript's
  `validateDocument`, `Document.parse` and `parseJson` hand over the module's
  copy of their input. The diagnostic memory probe's largest document allocates 224.3 MiB
  beyond a handed-over buffer, against 256.0 MiB beyond a borrowed one;
  through the TypeScript package its validation adds 247.2 MiB to Node's
  resident memory, from 278.5 MiB (351.5 MiB at a 64 MiB limit, from
  416.5 MiB).
- Exact-input refusals say what the parser expected or found and the byte where:
  `InputError`'s `Display` and TypeScript's `InputFailure.message` read, for
  example, `expected a colon after the member name at byte 5`. Codes, kinds
  and byte offsets are unchanged.
- OBI-10 findings name the meta-schema keyword whose value fails and point at
  that value (`the value of the type keyword does not satisfy the JSON Schema
  2020-12 meta-schema`), one finding for each failing keyword; a schema that is
  neither an object nor a boolean is reported at the schema (`a schema must be
  a JSON object or boolean`). Rule evidence is unchanged.
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
  path's dynamic scope, and cooperative cancellation. It bundles each value contract
  into one resource, so every reached schema compiles once, and reuses a
  resource set's identity index instead of indexing its resources again. A
  value contract that reaches a resource that was not supplied prepares with an empty
  stand-in for it, and a pass that reaches the stand-in gives no verdict
  (`resource-unavailable`) naming it. A value contract in which an
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
  evaluation), bounded from the value contract before a value is evaluated, the stack
  a pass of a value contract holding an unevaluated keyword may use as it evaluates
  (half, measured as it goes: a bound on bytes, so the depth it admits varies
  with the build and the host) and the unevaluated walk limit below, so a host
  that gives a larger thread admits deeper values and longer walks; each
  pattern's size, form
  and nesting, and the forms kept for all of a value contract's patterns together,
  counted as each is written, with each distinct `pattern` form kept once
  (`Limits::total_pattern_bytes`, TypeScript `totalPatternBytes`, 4 MiB,
  `pattern-total-byte-limit`); the regex crate's memory for all of a
  value contract's compiled patterns together (`Limits::total_pattern_memory`,
  TypeScript `totalPatternMemory`, 64 MiB, `pattern-total-memory-limit`, whose
  message states the arithmetic) for each distinct `pattern` form the run
  matcher does not match and each `patternProperties` name: a quarter, at most
  2 MiB each, for their search caches (`dfa_size_limit`) in equal parts, and
  the rest for their compiled forms (`size_limit`), each first within 48 KiB,
  with the forms that need more sharing what is left, at most 32 MiB each, so
  small patterns elsewhere change a large one's outcome only through their
  48 KiB each, no outcome depends on the order strings reach the patterns, a
  value contract compiling more than 1,024 patterns refuses before compiling any, 40
  counted-repetition names that peaked at 1.1 GB refuse in about 0.2 s, and a
  `pattern` value compiled when a string first reaches it stays within what it
  is given; the memory dynamic specialization adds to a value contract's program,
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
  process with an operating-system memory limit. Two limits are the
  application's to set and are off by default: `Limits::evaluation_steps`,
  which bounds only time, caps each pass's work, without which time can grow
  exponentially with some schemas, and `Limits::schema_resources` (TypeScript
  `schemaResources`) caps the schema resources one value contract may reach, refusing
  with `LimitExceeded` and `schema-resource-limit` before the library compiles
  the value contract, which bounds that compilation's time and memory (a
  99,990-resource `$id` chain: 11.5 s and 723 MB without a cap, 1.2 s and
  502 MB refused at 1,000). Exact `multipleOf` divides within a fixed
  arithmetic bound: past 4,096 significant digits or an exponent of ±20,000 in
  the value or the divisor, a division the answer needs gives no verdict
  (`numeric-arithmetic-limit`). A keyword that reads as text a string or
  member name holding an unpaired UTF-16 unit, such as `maxLength`, `pattern`
  or `propertyNames`, gives no verdict (`lone-surrogate-instance`). A string
  that reaches a `pattern` the evaluator does not match gives a no-verdict
  located at that keyword's original schema location. The default
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
- Evaluator messages and the documentation use the specification's term *value
  contract*, never an unqualified *contract*, which the specification reserves for
  an operation's whole contract: a failure reads "value does not satisfy the value
  contract", and limit refusals read "this value contract ...".
- Runnable first-use and lifecycle examples, legacy Go and TypeScript migration
  guidance, API reference documentation and reproducible performance measurements.

This first release remains unpublished. Package and specification versions are
independent.
