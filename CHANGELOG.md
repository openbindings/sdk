# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- Value contracts come from the operation. A conformant operation's `input()` and
  `output()` return a `ValueContract`; `prepare(&evaluator)`, `prepare_with(&evaluator,
  &resources)` and `prepare_with_control` borrow the evaluator and return
  `Result<PreparedContract, NoVerdict>`. `PreparedContract::validate_json` checks exact
  JSON text. TypeScript mirrors this with `ConformantOperationView.input()`/`output()`
  and `ValueContract.prepare(options)`.

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
  cooperative cancellation, declared work limits and selectable diagnostics.
- An evaluator integration trait and a reusable qualification suite for custom
  implementations.
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
