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
- `Conformant<Document>` (TypeScript `ConformantDocument`), the proven state, with
  infallible views, operation and alias lookup, exact schema access, reference
  inspection and opaque kind comparison. It is the only route to value contracts.
- Explicit immutable resources and reusable input/output value contracts, with
  preparation refusals separate from satisfies, fails, undefined or no-verdict outcomes.
  Undefined results carry a code, message and optional original schema location;
  no-verdict describes inability to answer.
- An optional JSON Schema 2020-12 evaluator with exact numeric semantics,
  cooperative cancellation and declared work limits. Preparation bounds schema
  compilation at 65,536 work units plus 64 for each schema position the contract
  reaches, refusing with `preparation-work-limit` beyond it; without a reachable
  dynamic anchor, a schema reached along many reference paths compiles once.
  Preparation reuses a resource set's identity index instead of indexing its
  resources again.
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
  explicit host-policy responsibilities.
- Provisional Rust provider traits and checked callers for source inspection,
  interface synthesis, binding invocation and operation invocation.
- A TypeScript API with bundled Wasm, exact-value editing, disposable owners and
  Fetch discovery for qualified browser, Node ESM and Worker hosts.
- Runnable first-use and lifecycle examples, legacy Go and TypeScript migration
  guidance, API reference documentation and reproducible performance measurements.

This first release remains unpublished. Package and specification versions are
independent.
