# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- Value contracts come from the operation. A conformant operation's `input()` and
  `output()` return a `ValueContract`; `prepare(&evaluator)`, `prepare_with(&evaluator,
  &resources)` and `prepare_with_control` borrow the evaluator and return
  `Result<PreparedValueContract, NoVerdict>`. `PreparedValueContract::validate_json` checks exact
  JSON text. TypeScript mirrors this with `ConformantOperationView.input()`/`output()`
  and `ValueContract.prepare(options)`, whose ready result carries its
  `PreparedValueContract` as `valueContract`.

- One exact OpenBindings 0.2 `Document` for parsing, inspection and in-place
  editing: atomic batches of JSON Pointer edits and typed part maps, and typed
  field and part setters, all keeping untouched bytes exact. Validation with original source locations, bounded
  findings and per-rule evidence.
- Document validation's fixed checks (OBI-02, OBI-10) and reference inspection use
  the unmodified jsonschema 0.58.6 library with its default pattern engine, which
  matches every fixed pattern on a finite automaton, exactly as ECMAScript does.
  The checks have no work budget; their work is linear in the admitted document,
  and a 60 MB SemVer near miss reaches its verdict in about 0.6 seconds. Core's own
  dependency graph holds neither renamed fork; the default evaluator keeps both.
- `Conformant<Document>` (TypeScript `ConformantDocument`), the proven state, with
  infallible views, operation and alias lookup, exact schema access, reference
  inspection and opaque kind comparison. It is the only route to value contracts.
- Explicit immutable resources and reusable input/output value contracts, with
  preparation refusals separate from satisfies, fails, undefined or no-verdict outcomes.
  Undefined results carry a code, message and optional original schema location;
  no-verdict describes inability to answer.
- An optional JSON Schema 2020-12 evaluator with exact numeric semantics,
  cooperative cancellation and declared work limits. Preparation charges schema
  compilation by the work it does and refuses with `LimitExceeded` and
  `preparation-work-limit` beyond 250,000,000 units (about 250 ms of compilation,
  or 175 to 250 MB of its memory, in the reference build) plus 4,096 for each
  schema position the contract reaches. Without a reachable dynamic anchor, a
  schema reached along many reference paths compiles once. Preparation reuses a
  resource set's identity index instead of indexing its resources again.
  `Limits::schema_resources` (TypeScript `schemaResources`) caps the schema resources one
  contract may reach, whose registration takes time quadratic in their number;
  a contract reaching more refuses with `LimitExceeded` and
  `schema-resource-limit` before registration. There is no cap by default.
- Cheap checks and explanations are chosen per call. `PreparedValueContract::validate`
  (and `validate_with_control`, `validate_json`) reports a failure without
  problems; `explain` and `explain_with_control` return the same verdict with the
  library's first error, redacted, with original schema facts. TypeScript
  `PreparedValueContract` gains `explain` beside `validate`. A prepared value contract keeps a
  compact copy of the keywords it compiled, with their original locations, and no
  longer keeps its document, supplied resources or location map.
- `operation::Invoker::with_validation_detail(Detail::Explain)` asks the invoker to
  explain a rejected value before withholding it, so `ValidationFailure::outcome()`
  carries the evaluator's problems; by default it carries the verdict only.
- An evaluator integration trait, whose `PreparedSchema::validate` receives the
  call's `Detail` (`Outcome` or `Explain`), and a reusable qualification suite
  that checks and explains every value, including explanations interrupted by
  cancellation.
- HTTP discovery and publication with bounded bodies, attempt deadlines and
  explicit host-policy responsibilities. Publication serves the well-known path
  when a request spells an unreserved character percent-encoded, as in
  `/.well-known/%6Fpenbindings` (RFC 3986 section 2.3), and never decodes a
  reserved escape such as `%2F` into a path separator.
- Provisional Rust provider traits and checked callers for source inspection,
  interface synthesis, binding invocation and operation invocation.
- A TypeScript API with bundled Wasm, exact-value editing, disposable owners and
  Fetch discovery for qualified browser, Node ESM and Worker hosts.
- A bundled Wasm module whose bytes do not depend on where it was built. The
  build gives the builder's Cargo home, toolchain, target directory and checkout
  fixed names in the module, and refuses a module that still embeds one; CI
  rebuilds it from another checkout path and Cargo home and requires identical
  bytes.
- Runnable first-use and lifecycle examples, legacy Go and TypeScript migration
  guidance, API reference documentation and reproducible performance measurements.

This first release remains unpublished. Package and specification versions are
independent.
