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
  New binding/source/dependency/example views follow this same exact-getter rule;
  their frozen metadata is plain data. A traversal cursor and each yielded owner
  have independent lifetimes. Scoped editable drafts own their converted exact
  leaves; caller-inserted exact handles stay caller-owned. Explicit draft disposal
  releases converted leaves; abandoning an undisposed draft does not revoke leaves
  that are still reachable. Individual leaf finalizers provide best-effort cleanup.
  Byte/text copies are ordinary host values. Dispose owners deterministically; release
  does not promise lower process RSS or reduced Wasm capacity.
- Preparation cache capacity counts entries, not bytes. Its default is four; zero
  disables implicit retention. Caller-retained contracts survive context drop and
  eviction. Resource sets are immutable, explicit and scoped to each context; URI
  identity never authorizes I/O. Core and default schema evaluation do no acquisition.
  For a missing resource, Rust `ParsedDocument::references()` and TypeScript
  `ParsedDocument.references()` expose explicit reference spellings and keyword
  locations; apply an application disclosure policy before logging these source facts.
- `PreparedContract::resource_completeness()` borrows evaluator-declared Complete,
  Incomplete evidence, or Undeclared. Existing custom `PreparedSchema` implementations
  default to Undeclared. TypeScript's bundled evaluator exposes the complete/incomplete
  states through a frozen cached `resourceCompleteness` getter with no additional
  Wasm owner. Complete is a resource claim, not a promise that every value is decidable.
  Evidence names one missing reference of this contract and follows diagnostic
  disclosure policy. Require Complete before replacing an active snapshot when your
  application needs that policy; deliberate partial previews can accept Incomplete.
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
unspecified settings. Existing Rust `ValueProblem` literals must also add
`details: None`; `ValueOutcome` construction is unchanged.
Custom evaluator authors can call `EvaluationProgram::original_location_bounded`
and match `LocationBudgetExceeded` separately from an unmapped location; its
point-of-definition Rustdoc demonstrates the complete match.

Rust adapter authors can call `SchemaRequest::evaluation_bounds(control)` for the
qualified static-reference fragment. The opaque owned `EvaluationBounds` exposes
`lower_program()`, `upper_program()`, `unavailable()` and consuming `into_parts()`
in `(lower, upper, unavailable)` order. **Neither closed projection alone is an
equivalent of the original partial schema.** Adapters must apply the paired verdict
rule, preserve incomplete-pass refusals and keep lower synthetic failures private.
The [proof and admission contract](partial-resource-bounds.md) covers influence,
identity, immutable ownership, combined text/graph limits and diagnostic scope.
Core custom-evaluator dispatch and strict `evaluation_program()` stay unchanged.
No TypeScript application API or configuration variant is added.

## Inspection and exact editing additions

Rust `ParsedDocument` enumerates and looks up retained binding, source and
dependency views. `OperationView` retains its existing binding-key association,
adds tags/deprecation, and exposes named example views. Borrowed fields are tied
to their view; clone a view or retain its `JsonRef` with `to_owned()` when needed.
Namespace enumeration distinguishes absent from empty, and field accessors reject
malformed metadata with original locations. It does not assert conformance or
resolve source availability. TypeScript projects whole namespace metadata eagerly
and checks every row; a keyed view checks only its selected metadata when read.
Existing global interpretation prerequisites still apply.

TypeScript exact member/element traversal uses one lazy disposable iterator
convention. `ParsedDocument.toDraft()` creates an `OwnedDocumentDraft` whose
`EditableDocumentDraft` value provides writable normative maps/lists and exact
opaque leaves. The existing readonly-friendly `DocumentDraft` input and
`authorDocument` function remain. Conversion, editing, building and normative
assessment are separate; no operation-reference rewriting or implicit acquisition
is introduced. See the shipped [inspection/edit caller](../packages/typescript/examples/inspect-edit.ts).

This alpha API update adds required `tags` and `deprecated` fields to TypeScript
`OperationMetadata`. Callers constructing metadata objects must provide them,
using null for absence. The closed `AuthoringErrorCode` union adds `invalid-field`
and `duplicate-members` for source conversion; exhaustive switches must handle
those cases. `AuthoringFailure.sourceLocation`, when present, addresses original
source bytes and never substitutes for a handwritten draft pointer. Existing Rust
method signatures stay unchanged; downstream extension traits with newly occupied
method names may need explicit qualification.

Document findings now deduplicate explanations for the same source occurrence
before presentation caps. Eliminating a repeat does not mark truncation; omitting
a distinct finding does. Equal schema text at different original locations still
produces distinct diagnostics. Repeated member names point to their offending
key tokens, including distinct byte offsets at an otherwise identical pointer.
Per-rule evidence remains independent of finding presentation. Value-problem
diagnostics are a separate unchanged contract.

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


Optional default-evaluator schema facts use
`DefaultEvaluator::new().with_schema_details(true)` (TS
`contracts({ includeSchemaDetails: true })`). They retain original source snapshots
and share the diagnostic string-byte budget. `ValueProblemDetails::Truncated`
sets result completeness false; absent details mean disabled or unavailable.
Exact bounds and enum choices remain JSON token strings. Existing Rust evaluator
implementations constructing `ValueProblem` must add `details: None` to preserve
their previous output. The enum is non-exhaustive; match future variants safely.
See the [evaluator resource contract](../crates/openbindings-json-schema-evaluator/README.md).
