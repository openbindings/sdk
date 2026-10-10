# API reference and compatibility contracts

Definition-level Rustdoc and JSDoc cover the public SDK surface. Start with the
[Rust first-use guide](rust-first-use.md) or [TypeScript guide](../packages/typescript/README.md)
for a complete job; use the definitions for exact result, ownership and limit contracts.

| Surface | Definition source | Reference generation |
| --- | --- | --- |
| Rust document/authoring/contracts | [`openbindings`](../crates/openbindings/src/lib.rs), including its exact-value reexports | `cargo doc --locked --no-deps -p openbindings` |
| Rust default evaluator | [`openbindings-json-schema-evaluator`](../crates/openbindings-json-schema-evaluator/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-json-schema-evaluator` |
| Rust evaluator contract kit | [`openbindings-schema-evaluator-test-support`](../crates/openbindings-schema-evaluator-test-support/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-schema-evaluator-test-support` |
| Rust HTTP discovery | [`openbindings-http-discovery`](../crates/openbindings-http-discovery/src/lib.rs), including native adapter | `cargo doc --locked --no-deps -p openbindings-http-discovery --features native` |
| TypeScript core and HTTP discovery | [`internal.ts`](../packages/typescript/src/internal.ts), as exported by the two entry points | Build the package, then run `node packages/typescript/scripts/check-api-reference.mjs target/api-reference` |
| Compiled Wasm asset import | [`wasm-module.d.ts`](../packages/typescript/src/wasm-module.d.ts) | Host import type documented alongside `initialize` |

The TypeScript renderer reads emitted declarations, follows reexports, and includes
inherited SDK members. It writes HTML plus a machine-readable inventory. The
`openbindings-internal-json` backend/numeric helpers and generated Wasm bindings
are implementation details, not additional supported SDK entry points. Exact JSON
contracts reexported by `openbindings` remain documented at their original definitions.
Rust's standard trait implementations inherit their standard contracts; the evaluator
and transport traits document their additional SDK obligations explicitly.

## Boundaries to preserve

- Parsing and authoring create exact snapshots. Normative assessment evaluates OBI
  document rules; only its conformant branch creates a `ValidatedDocument` proof.
  Preparing and validating a selected draft schema does not establish whole-document
  conformance. Context construction checks the operation namespace, including unrelated
  malformed aliases; unrelated metadata may remain draft data.
- The semantic partitions `Evidence`, `Conformance`, `ValueOutcome`,
  `ContractPreparation` and `Side` are closed, supporting exhaustive handling.
  Interpretation and no-verdict cause families are extensible. Rust matchers need a
  fallback for these causes. TypeScript exposes the known reason union for its
  version and is distributed with the matching Wasm engine; it does not add an
  arbitrary-string or unknown-outcome branch. Public report/problem records remain
  constructible by custom evaluator authors.
- Rust borrowed views do not create new owners. Rust clones and TypeScript `retain()`
  share immutable state. TypeScript `ParsedDocument.value`, `OperationView.value`,
  `ExactJson.get()` and `ExactJson.at()` create **new disposable owners** on each call.
  Byte/text copies are ordinary host values. Dispose owners deterministically; release
  does not promise lower process RSS or reduced Wasm capacity.
- Preparation cache capacity counts entries, not bytes. Its default is four; zero
  disables implicit retention. Caller-retained contracts survive context drop and
  eviction. Resource sets are immutable, explicit and scoped to each context; URI
  identity never authorizes I/O. Core and default schema evaluation do no acquisition.
  For a missing resource, Rust `ParsedDocument::references()` and TypeScript
  `ParsedDocument.references()` expose explicit reference spellings and keyword
  locations; apply an application disclosure policy before logging these source facts.
- Limits state their units and defaults in definitions. Cancellation is cooperative,
  not a deadline or preemptive interrupt. A same-thread `AbortSignal` cannot run while
  synchronous Wasm occupies that thread. HTTP discovery can interrupt asynchronous
  acquisition and retains observed response metadata on later body failure.
- Findings retain original byte coordinates. JSON Pointer escaping and safe text
  presentation are separate operations. Byte columns require conversion for UTF-16
  editors. A report's independent rule evidence survives finding-count and aggregate
  pointer-byte caps; diagnostic truncation must remain visible.

The value diagnostic byte contract, scratch allowances and conservative serialized
bounds are specified in the [evaluator guide](../crates/openbindings-json-schema-evaluator/README.md).
`Limits::diagnostic_bytes` and TS `EvaluatorLimits.diagnosticBytes` are the only
new evaluator limit. Adding a Rust `Limits` field is an intentional prerelease
struct-literal source change; callers should use `..Limits::default()` for
unspecified settings. `ValueProblem` and `ValueOutcome` construction is unchanged.
Custom evaluator authors can call `EvaluationProgram::original_location_bounded`
and match `LocationBudgetExceeded` separately from an unmapped location; its
point-of-definition Rustdoc demonstrates the complete match.

## Maintained verification

After generating the current package (`npm run build:wasm` then `npm run build` in
`packages/typescript`), run from the repository root:

```sh
python3 tools/verify-api-reference.py target/api-reference
node --test packages/typescript/test/api-reference.test.mjs
```

The standalone verifier denies missing Rust reference documentation and broken
intra-doc links, compiles Rustdoc examples, checks actual emitted TypeScript exports
and public/inherited members, and strictly compiles the reference consumer.
Component CI requires the Rust checks through
`tools/verify.py` and the TypeScript checks after building the package. Pass
`--rust-only` or `--typescript-only` to run the corresponding half.
A docs-only developer check may emit declarations with `tsc -p
packages/typescript/tsconfig.json` using existing generated binding declarations;
that checks types and documentation, not a source-bound Wasm artifact.

The negative controls remove documentation from temporary emitted copies of a
reexported type, a coordinate field and an inherited disposal method. Each must be
rejected. Definition presence is an omission gate, **not a quality percentage**.
Review rendered pages and compiled examples for truthful stage distinctions,
actionable errors, ownership, defaults/units, capability limits and retention. The
release checklist records those SDK-specific outcomes separately from source landing.
