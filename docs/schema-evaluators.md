# Choose a schema evaluator

Validate a document with `ParsedDocument::validate()` using only `openbindings`.
To validate an operation's values, select `DefaultEvaluator` from
`openbindings-json-schema-evaluator`, or supply your own `SchemaEvaluator`.
The [Rust first-use example](rust-first-use.md) shows the complete path.

## Without an evaluator

Core parses and preserves JSON, validates the fixed document rules, reads typed
validated fields, authors documents, resolves operation names and inspects
references. Its fixed document-schema and meta-schema checks are included.
They do not evaluate examples or runtime values against an operation's schemas.

Creating `ValueContracts` requires an explicit evaluator and resources. There is
no fallback value evaluator in core. A stated `true`, `false` or `{}` schema
still requires an evaluator. Absence alone means no value contract is stated.

`operation::Invoker::prepare` accepts operations with no stated value contracts
without an evaluator. Any stated input or output value contract without one yields
`ERR_SCHEMA_UNRESOLVED` during preparation, before provider effects. Supply one
through `with_evaluator` to enable those checks.

Plain `operation::Invoker::invoke` prepares each direction lazily when a value
crosses it. With no evaluator, the first value in a declared direction yields
`ERR_SCHEMA_UNRESOLVED`; a direction carrying no values triggers no preparation.
Provider effects can precede an output refusal. Use `prepare` when the application
requires both stated value contracts to prepare before invocation. The TypeScript facade bundles
the default evaluator; it does not expose custom evaluator callbacks.

## Bring your own

Implement `SchemaEvaluator::prepare` and return an owned `PreparedSchema`.
Pass the implementation in an `Arc` and an immutable `ResourceSet` to
`ValidatedDocument::value_contracts`. Implement `PreparedSchema::validate` to
return satisfies, fails, undefined or no-verdict. Observe `WorkControl` during both stages
and retain everything needed after the document and context owners are dropped.

`SchemaRequest` gives the original document, entry, source location and supplied
resources. Its `evaluation_program()` helper supplies the OpenBindings embedding
projection when the evaluator needs a closed JSON Schema program. It can refuse;
preserve the reason rather than claiming a value failure. The original context
remains available to an implementation with a different resolution strategy.
Reference acquisition is application policy, never an implicit request from core.

Implement OpenBindings' value-contract semantics, including its JSON Schema
provisions and original-source locations. Declare unsupported capabilities with
no-verdict. Neither an unsupported feature nor an exhausted budget establishes
an undefined result. Return `ValueOutcome::Undefined { detail }` only with
semantic provenance and disagreeing admissible completions. Construct its public
`UndefinedResult` with `new(code, message)` and attach an established original
schema location with `with_location(location)`, or set its public fields.
Preparation success does not promise a verdict on every
value, and missing diagnostic detail does not weaken an established failure.

Run the [boolean evaluator example](../crates/openbindings-schema-evaluator-test-support/examples/boolean_evaluator.rs)
for a minimal implementation. It supports boolean schemas and explicitly refuses
other forms. Qualify a broader implementation with
[`openbindings-schema-evaluator-test-support`](../crates/openbindings-schema-evaluator-test-support/README.md).
The suite distinguishes permitted refusals from wrong verdicts and checks that
reported locations exist in the original sources.
