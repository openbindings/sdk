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

Preparing a `Schema` requires an explicit evaluator. There is
no fallback value evaluator in core. A stated `true`, `false` or `{}` schema
still requires an evaluator. Absence alone means no schema is stated.

`DefaultOperationInvoker::new(provider, evaluator, external_schemas)` makes the invocation
dependencies explicit. Lazy invocation prepares a declared direction when its
first value crosses that boundary. An absent schema invokes no evaluator and
makes no validation claim. Provider effects can precede an output refusal.
Use `invoker.prepare(interface, name, &control)` when the application requires both
stated schemas to prepare before invocation. The resulting `PreparedOperation`
owns its validators and does not retain a provider or evaluator. See the
[invocation lifecycle](invocation.md) for binding selection and prepared reuse.
The TypeScript facade bundles
the default evaluator; it does not expose custom evaluator callbacks.

## Read an explanation

`prepared.explain(&value)` uses the same verdict as `evaluate`, then collects
established failures at original instance and schema locations. The default
evaluator returns at most 100 problems. Check `problems_complete`: false means
some reportable evidence may be missing because of this cap, a work/storage limit,
cancellation, or an unavailable original witness. It does not weaken the verdict.
The cheap `evaluate` call returns no problems and sets this flag to false.

For `allOf`, problems come from the branches established to fail. For failed
`anyOf`, `oneOf` or `not`, the problem names that applicator; it does not list
branch assertions as independent failures. The default evaluator includes
schema facts in `details`: missing required names, expected types, exact numeric
and size bounds, and complete enum choices. Bounds and choices preserve original
JSON token text. Missing members at tied locations keep required-array order.
Keywords without a matching variant, such as `const`, `pattern`, `multipleOf`
and `additionalProperties`, have no details. Pattern text stays out. Facts count
against the diagnostic storage allowance; facts that do not fit or cannot be
represented exactly are omitted and leave `problems_complete` false. An enum
list is always complete when present.

Messages stay fixed SDK text, and `Debug` redacts facts. Rejected instance values
are never copied into facts or messages. Serializing whole problems into a public
response sends schema facts. An app with a private contract should forward only
`instance` and `code`.

## Bring your own

The default evaluator is built on the SDK's internal crates as shared
implementation. Third-party evaluators use the public `SchemaEvaluator` contract;
`openbindings-schema-evaluator-test-support` qualifies that public contract.

Implement `SchemaEvaluator::prepare` and return an owned `SchemaValidator`.
Pass a borrowed implementation, and an immutable `ExternalSchemas` when resources are
supplied, to a conformant operation's `Schema::prepare`. Implement
`SchemaValidator::evaluate` to return satisfies, fails, undefined or no-verdict.
Each call passes a `Detail`. With `Detail::Outcome`, the caller's cheap check, a
failure carries no problems and `problems_complete` is false. With
`Detail::Explain`, a failure carries at least one problem unless detail work was
cut short or found no problem it can report in the original sources, in which
case `problems_complete` is false; cutting detail work short
never turns an established failure into another outcome. Both details reach the
same verdict. Observe `WorkControl` during both stages and retain everything
needed after the document, resources and request are dropped, including what
explanations read.

`EvaluationRequest` borrows the selected original `Schema` and `ExternalSchemas`.
`environment(limits, control)` returns an owned `SchemaEnvironment` with original
sources, inventories and typed reference queries. The bounded environment is
reused for a live document/resource pair; it does not rewrite the document or
fetch missing content. An implementation can retain this owned environment or
copy the prepared facts it needs. A failed environment query remains a typed
preparation/resolution cause, not a value verdict. `ExternalSchemas` retains the
supplied original JSON. Environment lookup can refuse when an unknown dialect
leaves its resource names uncertain; it does not infer that an undiscovered name
is absent. A certificate for a known dialect can establish its root identity
without establishing support for evaluating that dialect.

The default evaluator interprets standard draft-07, 2019-09 and 2020-12 external
resources, plus supported 2020-12 vocabulary combinations. A custom dialect is
selected through its original meta-schema and that meta-schema's own standard
2020-12 interpretation. Consumer schemas must satisfy the actual meta-schema,
including its authored restrictions and referenced helpers. Required unknown
vocabularies, unstable resource selection and cyclic admission obligations produce
typed preparation failures. Optional known vocabularies remain active; omitted
vocabularies do not turn their keywords into schema positions.

Required format assertion supports the 19 standard 2020-12 attributes. Optional
format assertion remains annotation-only. An unknown required attribute refuses
preparation. The `regex` format uses the SDK's documented ECMA-262 11th-edition
no-flags syntax policy, including Annex B; it checks syntax without executing the
expression. This format policy is separate from Unicode-mode `pattern` matching.
Date/time attributes check [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339)
syntax, calendar dates, offsets and UTC month boundaries for leap-second syntax.
They do not consult an external table of historical or announced leap seconds.

`PreparationLimits` keeps inventory footprint (`max_schema_nodes`,
`max_identity_bytes`) separate from cooperative SDK work and explicit storage
reservations (`max_sdk_work_units`, `max_sdk_reservation_bytes`). Work is optional
and off by default (`None`); the caller chooses a time bound through `WorkControl`
cancellation or `Some(work_units)`. Storage keeps its 512 MiB default allowance
for memory held at once. The dated [host measurement](performance-boundaries.md#limits-principle-november-2026)
gives a work-unit suggestion for about 250 ms on that host. A completed cached
inventory still checks the requested footprint bounds; work and storage already
shared are not charged again. Nested preparation cannot renew a spent allowance,
and cancellation retains its distinct cause. A failed limit check can omit origin
details rather than allocating an unbounded diagnostic path.

The counters cover cooperative SDK sites in environment inventory, reference
queries, retained operands, semantic proofs and nested meta-schema validation.
Storage reservations include temporary and failed SDK construction while held.
Local syntax checks, traversal scratch and nested meta-evaluation scratch release
their reservations when dropped. Retained programs and indexes stay charged;
mixed construction estimates conservatively stay charged with their owner. They do not
count every dependency operation, allocator request or custom callback, and are
not deadlines or process memory ceilings. A custom evaluator must declare its own
resource scope. Dependency construction requires separate structural admission,
representation limits and measured build qualification. Pattern construction now
shares equivalent programs within one preparation Attempt. Whole-source resource,
performance and host qualification remains separate; older exact dependency-
accounting receipts do not define the current public contract.

On the Original path, the built-in evaluator's `Limits::evaluation_sdk_work_units` and
`Limits::evaluation_sdk_reservation_bytes` apply to each value invocation. Verdict
and diagnostic passes share its remaining allowance; repeated evaluations start
fresh. Default Outcome calls can instead use the compiled lane with static
admission and peak scratch accounting; any non-default limit selects Original.
See [execution paths](../crates/openbindings-json-schema-evaluator/README.md#execution-paths).
Required custom-meta evaluation debits both its evaluation allowance and
the enclosing preparation Attempt, so either exhaustion can return a preparation
Limit. Immutable pattern representation retains its separate limits. SDK work
and reservation units do not count all dependency work or physical allocation.

Implement OpenBindings' schema evaluation semantics, including its JSON Schema
provisions and original-source locations. Declare unsupported capabilities with
no-verdict. Neither an unsupported feature nor an exhausted budget establishes
an undefined result. Return `ValueOutcome::Undefined { detail }` only with
semantic provenance and disagreeing admissible completions. Build its
`UndefinedResult` with `new(code, message)` and attach an established original
schema location with `with_location(location)`. Refuse with
`NoVerdict::new(reason, code, message)`, adding `with_location(location)` when
the refusal has an original schema location.
Preparation success does not promise a verdict on every
value, and missing diagnostic detail does not weaken an established failure.

Construct a problem with `ValueProblem::new(instance, schema, code, message)`
and add schema facts with `with_details(details)`. `instance` is the failing
node's exact `Pointer` from the value the call received (its root is that value,
also when it is part of a larger text); `JsonValue::at_pointer` checks one and
`Pointer: FromStr` parses escaped text from a library. Name `schema` by resource
and exact pointer from that source's root. Use the original `Schema` and
`SchemaEnvironment` locations while preparing or evaluating; retain owned source
facts needed later. The borrowed request itself does not survive preparation.
`SchemaOrigin::new(source_uri, pointer)` takes an exact `Pointer`. If a keyword
cannot be mapped to an original source, use an established selected-schema origin
or omit the optional origin; never invent one for unavailable content. Callers
find line and column on demand with `JsonValue::locate`, `ConformantDocument::locate`
and `ExternalSchemas::locate`.

These records may gain fields in later versions, so they have no struct-literal
form outside the SDK; their fields stay public to read and assign. `ValueOutcome`
is a closed partition: build `ValueOutcome::Fails { message, problems,
problems_complete }` directly and match every variant.

Run the [boolean evaluator example](../crates/openbindings-schema-evaluator-test-support/examples/boolean_evaluator.rs)
for a minimal implementation. It supports boolean schemas and explicitly refuses
other forms. Qualify a broader implementation with
[`openbindings-schema-evaluator-test-support`](../crates/openbindings-schema-evaluator-test-support/README.md).
The suite checks and explains every value. It records capability refusals as gaps, distinguishes them from wrong verdicts, checks that both calls agree and that a cheap check reports
no problems, checks that reported schema locations exist in the original
sources and instance pointers in the value, and explains each failing value again
inside a larger text, which must not change its problems.
