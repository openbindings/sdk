# Changelog

## 0.2.0-alpha.1 (working draft)

### Added

- Rust `value_contracts(&validated)` in the default evaluator companion,
  `ValueContract::validate_json` for exact text admission, and
  `ValueContractPreparation::into_result` with a payload-preserving
  `ValueContractPreparationError` for error propagation.

- Exact OpenBindings 0.2 document parsing, typed authoring and validation with
  original source locations, bounded findings and per-rule evidence.
- Validated document views, operation and alias lookup, exact schema access,
  reference inspection and opaque kind comparison.
- Explicit immutable resources and reusable input/output value contracts, with
  distinct preparation results and satisfies, fails, undefined or no-verdict outcomes.
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
