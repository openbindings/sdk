# Choose a schema evaluator

Validate a document with `Document::validate()` using only `openbindings`.
To validate an operation's values, select `DefaultEvaluator` from
`openbindings-json-schema-evaluator`, or supply your own `SchemaEvaluator`.
The [Rust first-use example](rust-first-use.md) shows the complete path.

## Without an evaluator

Core parses and preserves JSON, validates the fixed document rules, reads typed
conformant fields, authors documents, resolves operation names and inspects
references. Its fixed document-schema and meta-schema checks are included.
They do not evaluate examples or runtime values against an operation's schemas.

Preparing a `ValueContract` requires an explicit evaluator. There is
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
Pass a borrowed implementation, and an immutable `ResourceSet` when resources are
supplied, to a conformant operation's `ValueContract::prepare_with`. Implement
`PreparedSchema::validate` to return satisfies, fails, undefined or no-verdict.
Each call passes a `Detail`. With `Detail::Outcome`, the caller's cheap check, a
failure carries no problems and `problems_complete` is false. With
`Detail::Explain`, a failure carries at least one problem unless detail work was
cut short or found no problem it can report in the original sources, in which
case `problems_complete` is false; cutting detail work short
never turns an established failure into another outcome. Both details reach the
same verdict. Observe `WorkControl` during both stages and retain everything
needed after the document, resources and request are dropped, including what
explanations read.

`SchemaRequest` gives the original document, entry, source location and supplied
resources. Its `evaluation_program()` helper supplies the OpenBindings embedding
projection when the evaluator needs a closed JSON Schema program. Its resolution
space is assembled once per live document revision and resource set and reused by
later preparations with that pair, until the set or the revision is dropped. It can refuse; preserve the reason rather than claiming a value
failure. The original document and resources remain available to an implementation
with a different resolution strategy.
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

Construct a problem with `ValueProblem::new(instance, schema, code, message)`
and set its public `details` to add schema facts. Locate `instance` with
`JsonRef::location()` on the failing node of the value the call received: its
exact pointer from that value's text, with byte offset, line and column. Name
`schema` by resource and exact pointer: `SchemaRequest::location(node)` builds
one for a node of the request's document or supplied resources, and
`entry_location()` names the selected schema; do this at preparation, since the
prepared object must not keep the request. `SchemaLocation::new(resource,
pointer)` takes a `Pointer` parsed from RFC 6901 text. When a keyword cannot be
mapped to an original source, name the selected schema rather than guessing.
Callers find a schema location's line and column with `Conformant::locate` or
`ResourceSet::locate`.

Run the [boolean evaluator example](../crates/openbindings-schema-evaluator-test-support/examples/boolean_evaluator.rs)
for a minimal implementation. It supports boolean schemas and explicitly refuses
other forms. Qualify a broader implementation with
[`openbindings-schema-evaluator-test-support`](../crates/openbindings-schema-evaluator-test-support/README.md).
The suite checks and explains every value. It distinguishes permitted refusals
from wrong verdicts, checks that both calls agree and that a cheap check reports
no problems, and checks that reported schema locations exist in the original
sources and that each instance location is its node's own.
