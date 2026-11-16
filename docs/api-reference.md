# API reference

The SDK follows the pinned OpenBindings 0.2 document and schema model. The packages
are unpublished; the migration does not preserve superseded SDK interfaces.
Start with the [Rust guide](rust-first-use.md) or [TypeScript guide](../packages/typescript/README.md).
Definition-level Rustdoc and JSDoc specify each method's ownership, result and limits.

For protocol implementations, read [writing a binding kind adapter](writing-a-kind.md).
It builds a complete adapter and explains the checked calls and kind-owned rules.

| Responsibility | Definition source | Reference command |
| --- | --- | --- |
| Exact documents, canonical entries, conformance and schemas | [openbindings](../crates/openbindings/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings` |
| Optional schema evaluator | [openbindings-json-schema-evaluator](../crates/openbindings-json-schema-evaluator/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-json-schema-evaluator` |
| Evaluator qualification | [openbindings-schema-evaluator-test-support](../crates/openbindings-schema-evaluator-test-support/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-schema-evaluator-test-support` |
| Shared context | [openbindings-context](../crates/openbindings-context/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-context` |
| Invocation | [openbindings-invocation](../crates/openbindings-invocation/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-invocation` |
| Schema identity and directional comparison | [openbindings-schema-comparison](../crates/openbindings-schema-comparison/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-schema-comparison` |
| Source pulling | [openbindings-source-pull](../crates/openbindings-source-pull/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-source-pull` |
| Kind description and source checking | [openbindings-kind-description](../crates/openbindings-kind-description/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-kind-description` |
| HTTP discovery | [openbindings-http-discovery](../crates/openbindings-http-discovery/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-http-discovery --features native` |
| TypeScript core and HTTP discovery | [core entry](../packages/typescript/src/index.ts), [discovery entry](../packages/typescript/src/http-discovery.ts) | Build, then run `node packages/typescript/scripts/check-api-reference.mjs target/api-reference` |
| TypeScript test observations | [testing entry](../packages/typescript/src/testing.ts) | Included in the TypeScript reference renderer |

The renderer follows actual exported declarations and inherited SDK members. It
checks documentation and rejects public signatures that import private Wasm bindings.
Private compiler graphs, scalar bridge records and lookup indexes are implementation
mechanisms, not additional public document models. Current interface implementations are Rust
APIs; the TypeScript facade does not expose their provider protocols.

For source pulling, `AddedEntry::uses` and `BindingProposal::uses` expose only
additions needed by binding data. `SchemaProposal` exposes the replacement
`schema`; an additional `uses` member is retained as unknown metadata. The
checked pull validates labels and all additions together. Binding `references`
and omitted `existing`, `alsoIdentifies` and `through` labels must name input
entries by primary key, with operation slots required to exist. A caller keeping part
of a pull follows schema references and each reached added binding's operation
and `uses`, as the [adapter guide](writing-a-kind.md#what-goes-in-uses) explains.

Both `SourcePullerExt::pull_source` and `pull_source_with_limits` return
`Result<(PullResult, ConformantDocument), PullError<Self::Error>>` through their
futures. The prospective document contains all additions, retains the original
JSON admission limits and leaves stale proposals unapplied. Adopting a selected
subset or applying a proposal still requires validation of that edited document.
`PullReport::from_json` and `PullResult::from_json` return a struct `CarrierError`:
`kind()` gives a Copy `CarrierErrorKind` and `pointer()` borrows an optional
failing row, such as `/existing/1` or `/report/existing/1`. Nested failures identify
the enclosing row; unrelated top-level failures need not have a pointer.

Kind-description owns its own `CarrierError` struct and Copy, non-exhaustive
`CarrierErrorKind`. Its `pointer() -> Option<&Pointer>` locates fixed fields,
nested disclosures and support indices relative to the admitted carrier; opaque
context failures locate `/context`. Both role crates keep labels inspectable
through `kind()` while omitting them from `Display` and `Debug`.

`PullValidationError::Relationship` carries a `RelationshipError` struct and
exposes it through `Error::source()`. Caller-built labels in `KindError::LimitExceeded`,
`RequestError::Shape`, `PullValidationError::LimitExceeded`, and
`ComparisonFailureKind::LimitExceeded` remain matchable but are omitted from formatting.
`kind()` returns a Copy, non-exhaustive `RelationshipErrorKind`; `pointer()` borrows
an optional exact provider-report row such as `/report/existing/2`. Conflicting
rows identify the later row. Missing coverage and additions with no report row
have no row pointer. `Display` does not include locations.

`CheckFailure<E>`, `KindError<E>`, `PullFailure<E>` and `PullError<E>` each expose
`map_error<F>(self, map: impl FnOnce(E) -> F)` returning the same family with `F`.
Only the generic error payload is transformed. Other variants and payloads pass
through unchanged, and their closures are not called. Checked source checks use
`error.map_error(|failure| failure.map_error(convert_native))` for the two layers.
The pull errors `RequestError`, `PullValidationError`, `PullFailure` and `PullError`
are non-exhaustive. Handle unknown failures by refusal or propagation.

`InterfaceIssueKind` and `operation::OperationPreparationFailure` are
non-exhaustive. Downstream matches need a fallback that refuses or propagates
unknown categories. `ReferenceResolution` and limit structs remain exhaustive.

The role crates export the pinned operation keys for authoring aliases and
`supports` claims:

| Crate | Constants |
| --- | --- |
| `openbindings-kind-description` | `LIST_KINDS`, `DESCRIBE_KIND`, `CHECK_SOURCE` |
| `openbindings-source-pull` | `PULL_SOURCE` |
| `openbindings-invocation` | `INVOKE_BINDING`, `PREFLIGHT_BINDING` |

Each string is the full `openbindings.<interface>.<operation>` key. These names
make an author's correspondence claim; they do not verify implementation behavior.

## Exact documents and conformance

Rust `JsonValue::parse_prefix(input)` and
`parse_prefix_with_limits(input, limits)` return `(JsonValue, end)` for the first
exact value. `end` is its exclusive byte offset, including leading whitespace
and excluding trailing whitespace. `JsonPrefixError::Incomplete` means its end
is unknown; `Invalid(InputError)` keeps the usual admission diagnostics.
A number ending at the buffer end is always incomplete. At known EOF, pass an
incomplete remainder to `JsonValue::parse_with_limits` to accept the final number
or obtain the ordinary refusal. Complete strings, containers and literals need
no lookahead. Only the value and constant lookahead are inspected; following
bytes, including invalid UTF-8, remain with the host. The byte allowance covers
the prefix through `end`; depth and node limits are unchanged.
An impossible prefix returns `Invalid` immediately. After `Incomplete`, retain
the buffer. For files or batches whose producers keep writing, append at least
the current buffer length before retrying, unless EOF or a limit is reached
first, to keep cumulative work linear. Waiting for that growth can stall a peer
that sends one message and waits for a reply. Retrying on every read avoids that
wait but reparses from the start each time and can make cumulative work quadratic.
For newline-delimited JSON or MCP over stdio, find the protocol delimiter and
parse the complete message once. The host owns separators, buffering and I/O.
See the tested
[whitespace-separated host example](../crates/openbindings/examples/json_sequence.rs).
The TypeScript facade does not yet expose this primitive.

Rust `JsonValue::semantic_eq(&other)` compares exact JSON values without
floating-point conversion. It returns `Some(true)` for `1e400` and `10e399`,
`Some(false)` for unequal values, and `None` if either value contains duplicate
object member names. Token spelling is preserved independently of value equality.

| Operation | Contract |
| --- | --- |
| Rust `Document::parse`, TypeScript `Document.parse` | Admit exact JSON and retain an immutable snapshot, including nonconformant drafts. Syntax, encoding and admission failures are separate from document findings. |
| Rust `Document::new`, TypeScript `Document.create` | Create a draft declaring the supported specification version and an empty operations map. |
| `Document::retained_bytes` / `Document.retainedBytes()` | Estimate current shared snapshot heap storage, including source, nodes and built caches. Charge clones and proofs once; excludes allocator overhead and Wasm memory growth. Later lookups can grow the estimate; see definition-level docs for cache growth. |
| `Document::value` / `Document.value()` | Inspect original JSON, including extension members, original number tokens and duplicate occurrences. TypeScript acquires a disposable owner. |
| `set`, `remove`, `edit` | Return a new immutable snapshot. An atomic batch addresses the original revision and preserves untouched bytes. Failure leaves all earlier owners unchanged. |
| `Document::validate` / `Document.validate()` | Assess all applicable OBI rules. Only established conformance creates a `ConformantDocument`. Unsupported declared versions have a separate refusal. |
| `validate_document` / `validateDocument` | Assess exact source directly, including syntax failures. Rust can return a proved document; TypeScript returns only a detached report. |
| `ConformantDocument::document`, `into_inner` | Borrow the retained draft snapshot, or consume the proof and recover it. |
| `ConformantDocument.asDocument()`, `intoDocument()` | Acquire a separate editable snapshot owner, or consume this proof. Consuming revokes aliases of this handle; independently retained owners survive. |

`ConformantDocument` is privately constructed and has no editing methods. Its domain
reads need no repeated OBI assessment. Raw document inspection provides evidence for
repair without exposing partly interpreted canonical entries. Diagnostic
`Document::references()` inspects a draft; use `proof.document().references()`
from a proof. It acquires no resources and proves neither document conformance
nor value decidability. Supplied resources use `references_with_resources` on
the same document snapshot.

Match `ReferenceReport::Complete { references }` or
`ReferenceReport::Incomplete { retained, limitation }` before reading observations.
The TypeScript union uses `complete: true` with `references` and `complete: false`
with `retained` and `limitation`. An incomplete inventory has no `references` field.
`ReferenceLimits { max_report_bytes }` selects retained text through
`references_with_limits` or `references_with_resources_and_limits`; TypeScript
uses `references({ limits: { maxReportBytes } })`. The default stays 8 MiB.

`Evidence` is `Met`, `Violated`, `Inconclusive` or `NotApplicable`. A document's
`Conformance` is conformant, nonconformant or undetermined. Findings explain rule
evidence; omitted findings do not erase an established violation. Use rule IDs,
codes and evidence for logic, and treat explanatory messages as display text.
`ValidationLimits { max_findings, max_pointer_bytes }` passed to
`Document::validate_with_limits` controls finding retention only. The defaults
remain 4,096 findings and 8 MiB of generated source-pointer text. TypeScript uses
`document.validate({ limits: { maxFindings, maxPointerBytes } })` and exposes
`validationLimits()` for the defaults. Its signature is
`Document.validate(options?: ValidationOptions): DocumentValidation`, with
`ValidationOptions { limits?: Partial<ValidationLimits> }`. Omitted options or
limits use the cached default assessment; unknown option names are rejected.
Zero allowances still assess every rule. Custom reports do not replace the
cached default report.

Plain Rust records whose fields are public data expose `code`
and `kind` as fields. Enums and types whose private fields protect an invariant
expose `code()` and `kind()` methods.

### Document finding codes

The [complete finding-code table](../crates/openbindings/README.md#document-finding-codes)
lists every stable `Finding.code`, its OBI rule and its meaning. It applies to
Rust and TypeScript conformance reports, including parsing and admission limits.
The former `input-limit` finding is now `byte-limit`, `depth-limit` or `node-limit`.
Every rule remains inconclusive and carries the parser offset (no pointer).
Messages describe the reached limit without API advice. To admit more, use
`validate_document_with_limits` with a larger `JsonLimits::max_bytes`, `max_depth`
or `max_nodes`, or `validateDocument` with `limits.maxBytes`, `maxDepth` or `maxNodes`.

OBI-02 findings add optional `keyword`, `schema: SchemaOrigin` and
`details: ValueProblemDetails`. The origin points into the published fixed schema
at `https://openbindings.com/schema/openbindings-0.2.0.json`. For example, a binding
missing `source` has keyword `required`, schema pointer `/$defs/BindingEntry/required`
and details `{ "kind": "required", "member": "source" }`. Each missing member gets
its own finding in required-array order. Type facts carry complete expected types;
numeric/size bounds and enum choices use exact schema JSON token strings.
Disallowed members are located by the finding pointer, with keyword
`additionalProperties` and no detail containing the rejected name or value.

Failed `anyOf` and `oneOf` stop at their applicator with no branch details, following
the value evaluator's policy. In particular, a schema that is neither object nor
boolean fails `/$defs/JSONSchema/anyOf`; guessing a branch would lose its constraints.
The pinned OBI-02 schema has no `oneOf`. `pattern` and `uniqueItems` have no matching
detail variant either. Guidance never copies document values or evaluates example
or caller values. If keyword guidance is unavailable, the generic finding remains.

`ValidationReport` retains at most 4096 findings and 8 MiB of source-pointer bytes
by default, adjustable through `ValidationLimits`.
The pinned 0.2 schema's largest complete serialized `[keyword, schema, details]`
tuple is 202 bytes in Rust JSON (201 in TypeScript JSON). An exhaustive schema
test guards this fixed property; there is no runtime fact-size cap. Count and
source-pointer omissions set `findings_truncated` (`findingsTruncated`), leaving
rule evidence intact.

## Canonical entries

Rust `Operation`, `Source`, `Binding`, `Dependency` and `Example` retain their
exact original declaration and the conformant document that proves it. Rust clones
share immutable storage; dropping a parent does not invalidate acquired children.

In TypeScript these names are readonly record types. Every canonical entry read
returns fresh, detached, runtime-frozen records containing only declared scalar
fields and frozen string arrays. They have no methods, handle, document reference,
cache or lazy fields, remain readable after proof disposal and need no disposal.
Navigation and exact payload queries use a live `ConformantDocument` and primary keys.
Reusing a key with another proof selects that proof's revision; records confer no
authority to query a disposed proof.

| Read | Meaning |
| --- | --- |
| `operations()` | Operations in lexical primary-key order. |
| `operation(key)` | Exact primary-key lookup. |
| `resolve_operation(name)` / `resolveOperation(name)` | Primary key or declared alias in the proved shared namespace. |
| `bindings()`, `sources()`, `dependencies()` | Optional canonical collections in lexical key order. Absence differs from present-empty. |
| `binding(key)`, `source(key)`, `dependency(key)` | Exact namespace lookup. |
| `schemas()`, `schema(key)` | Contextual named schema owners; reading performs no preparation. |
| Rust `Operation::input`, `output`; TypeScript `proof.inputSchema(key)`, `outputSchema(key)` | Optional contextual `Schema`; missing operations and absent fields yield `undefined` in TypeScript; false stays a schema. |
| Rust `Operation::bindings`; TypeScript `proof.operationBindings(key)` | Bindings in lexical order, without preference ranking. TypeScript returns `undefined` for a missing operation, an empty array for no bindings. |
| Rust `Binding::operation`, `source`; `Dependency::operation` | The related entry from the same snapshot. TypeScript uses `proof.operation(record.operationKey)` and `proof.source(binding.sourceKey)`. |
| Rust `Operation::examples`, `example`; TypeScript `proof.examples(operationKey)`, `example(operationKey, exampleKey)` | Examples with operation-local keys, without instance validation. Missing entries or absent examples yield `undefined`; a declared empty collection stays empty. |
| Rust source/binding `data`, example `input`/`output`; TypeScript `sourceData`, `bindingData`, `exampleInput`/`exampleOutput` on the proof | Optional exact JSON owners. Missing entries or absent fields yield `undefined`; explicit null and wide number tokens remain exact values. |
| Rust entry `value`; TypeScript `operationValue`, `bindingValue`, `sourceValue`, `dependencyValue`, `exampleValue` on the proof | Exact original declarations including extension fields. TypeScript returns `undefined` for missing entries. |

The TypeScript record declarations are:

```ts
export interface Operation {
  readonly key: string;
  readonly description?: string;
  readonly deprecated?: boolean;
  readonly aliases?: readonly string[];
  readonly tags?: readonly string[];
}
export interface Binding {
  readonly key: string;
  readonly description?: string;
  readonly operationKey: string;
  readonly sourceKey: string;
  readonly preference?: number;
  readonly idempotent?: boolean;
  readonly deprecated?: boolean;
}
export interface Source {
  readonly key: string;
  readonly description?: string;
  readonly kind: string;
}
export interface Dependency {
  readonly key: string;
  readonly description?: string;
  readonly operationKey: string;
  readonly kinds?: readonly string[];
}
export interface Example {
  readonly key: string;
  readonly description?: string;
}
export declare function acceptsKind(dependency: Dependency, kind: string): boolean;
```

`acceptsKind` is pure and needs neither initialization nor a live proof. An absent
filter accepts any string, including an unpaired UTF-16 unit; a present filter uses
exact string equality. Non-string kinds throw `TypeError`.

The emitted `ConformantDocument` class includes these signatures. Every key is
primary except the argument to `resolveOperation`; example keys are local to the
named operation. All queries require a live proof and reject invalid scalar strings.

```ts
operations(): readonly Operation[];
operation(key: string): Operation | undefined;
resolveOperation(name: string): Operation | undefined;
bindings(): readonly Binding[] | undefined;
binding(key: string): Binding | undefined;
sources(): readonly Source[] | undefined;
source(key: string): Source | undefined;
dependencies(): readonly Dependency[] | undefined;
dependency(key: string): Dependency | undefined;
operationBindings(operationKey: string): readonly Binding[] | undefined;
examples(operationKey: string): readonly Example[] | undefined;
example(operationKey: string, exampleKey: string): Example | undefined;
inputSchema(operationKey: string): Schema | undefined;
outputSchema(operationKey: string): Schema | undefined;
operationValue(key: string): ExactJson | undefined;
bindingValue(key: string): ExactJson | undefined;
sourceValue(key: string): ExactJson | undefined;
dependencyValue(key: string): ExactJson | undefined;
exampleValue(operationKey: string, exampleKey: string): ExactJson | undefined;
sourceData(key: string): ExactJson | undefined;
bindingData(key: string): ExactJson | undefined;
exampleInput(operationKey: string, exampleKey: string): ExactJson | undefined;
exampleOutput(operationKey: string, exampleKey: string): ExactJson | undefined;
```

Optional scalar fields remain optional: `None` in Rust, `undefined` in TypeScript.
Absent boolean flags do not become false. Declared preference is an optional
interoperable integer, not an SDK preference type. Kind names compare exactly;
an absent dependency kind filter accepts every kind, while a present filter is
nonempty. Alias and tag arrays preserve declaration order; TypeScript freezes them.
OBI-01 proves scalar strings throughout a conformant document. Unpaired UTF-16
remains representable in raw JSON, findings and external instances, not in a proof
that would violate that rule.

TypeScript documents, proofs, schemas, exact JSON, resource sets and prepared schemas
remain independently disposable owners. Dispose each schema-map value and each exact
traversal cursor and yielded member/value. A forgotten `Schema` or `ExactJson` from
a document keeps its whole document in Wasm memory even after proof disposal, until
disposed or finalized. Entry records retain no Wasm storage. Finalizers are best
effort; disposal does not promise that process RSS or Wasm capacity shrinks.

TypeScript catches `WebAssembly.RuntimeError` at its Wasm boundary, including
allocation traps, and throws `SdkError` with code `wasm-trap` and fixed text:
`The OpenBindings WebAssembly instance trapped and can no longer be used.` The
module instance becomes unusable. All later SDK operations fail immediately with
that code; `initialize()` rejects and cannot replace it. Disposal remains safe
and idempotent, skips Wasm after a trap, and finalizers never throw. The original
runtime error is not attached as a cause. Resume work in a fresh module instance.
A rejection from the caller's initialization promise, even a
`WebAssembly.RuntimeError`, is a retryable `wasm-initialization` error with its
cause retained.

## Schemas and value results

A `Schema` owns its original location and conformant context. Rust
`PreparationLimits::max_sdk_work_units`, `ComparisonLimits::max_work` and
`PullLimits::max_relationship_work` are `Option<usize>`, all `None` by default.
A time bound belongs to the caller through `WorkControl` cancellation or
`Some(work_units)`. The corresponding memory allowances keep 512 MiB, 64 MiB
and 64 MiB defaults, measuring conservative reservations for storage held at once.
Dropped scratch is released; retained programs, indexes and reports stay charged.
See the dated [host-specific 250 ms preparation suggestion](performance-boundaries.md#limits-principle-november-2026).

`schema.prepare(&evaluator, &external)` uses default preparation limits and no
cancellation and returns
`Result<PreparedSchema<E::Validator>, PreparationFailure>`. Use
`prepare_with_control(&evaluator, &external, &limits, control)` for explicit limits
or cancellation. TypeScript
`schema.prepare({ resources, limits, signal })` returns a `ready` branch owning a
`PreparedSchema`, or a `refused` branch with a `PreparationFailure`. Preparation
failure judges no instance. A missing operation or absent schema is found before
preparation and is not a preparation failure.

Rust `PreparedSchema::retained_bytes(&self) -> Option<usize>` asks its validator
for an on-demand estimate of heap allocations it keeps alive. The defaulted
`SchemaValidator::retained_bytes` returns `None` for custom validators unless
they override it. The default evaluator returns `Some` on its SDK lane and
`None` on its compiled `jsonschema` lane, whose allocations are opaque.
`PreparedOperation::retained_bytes(&self) -> Option<usize>` adds the retained
document, operation name and present schema slots; any present unknown estimate
makes the result `None`. Absent slots add nothing. Shared allocations are counted
per owner, including across slots; these are neither resident-memory readings
nor bytes freed by dropping one handle. Allocator overhead, process-wide fixed
schemas and Wasm memory growth are excluded. Later caches can grow the estimate.
No accounting runs during preparation or evaluation. This API is Rust only.

Schemas received as interface data, including a kind description's `sourceData`
and `bindingData` and a context challenge's `schema`, can be prepared as their own
roots. Supply the exact schema as an `ExternalSchema { uri, document }` under a
URI the caller chooses, build `ExternalSchemas`, then use Rust
`resources.prepare(uri, &evaluator)` or
`resources.prepare_with_control(uri, &evaluator, &limits, control)`. TypeScript
uses `resources.prepare(uri, { limits, signal })`; this set supplies all resources
for the attempt. These entry points have the same preparation refusals,
cancellation behavior and prepared-owner lifetime as `Schema.prepare`. A URI
absent from the set returns a preparation `Resolution` failure with
`ResourceUnavailable`, never a value outcome. The URI is the resource's retrieval
identity for relative references and diagnostics; its own `$id` governs the base
when present. Other resources in the set remain available for references. Local
`#/$defs/...` references and `{"$ref":"#"}` start from this resource's root. No
synthetic OBI is needed, and core `Schema` retains its conformance proof.

Evaluator implementers receive `EvaluationRequest { root, external_schemas }`.
`SchemaRoot::Contained(&Schema)` selects a contained schema;
`SchemaRoot::Supplied(&str)` selects a resource by its retrieval URI in the set.
Use `request.environment(limits, control)` to obtain either original context,
including the selected root's base, dialect and provenance.

`evaluate` is the cheap value check; `explain` requests diagnostics for the same
semantic result. In Rust, first parse exact JSON text or bytes with
`JsonValue::parse` or `parse_with_limits`, then call `evaluate(&value)` or
`explain(&value)`. TypeScript `evaluateJson(input, options?)` admits and evaluates
exact JSON text or UTF-8 bytes; `explainJson(input, options?)` admits the same
input and returns `explain`'s bounded problems. Both return `JsonCheck` and
release temporary input storage within the call, leaving no input owner to
dispose. Both use the default admission limits (32 MiB, 10,000 container levels,
1,000,000 nodes), preserve exact numbers, and admit before observing cancellation.
Malformed JSON returns `input-error`; wrong input types and literal unpaired
UTF-16 text throw. For custom admission limits, use `parseJson` followed by
`explain` and dispose the parsed owner. TypeScript `evaluate` accepts checked ordinary JSON or an
`ExactJson`; a string there is a JSON string value, not serialized JSON text.

| Value result | Meaning |
| --- | --- |
| `Satisfies` / `satisfies` | Established satisfaction of the selected schema. |
| `Fails` / `fails` | Established failure; diagnostic incompleteness does not weaken it. |
| `Undefined` / `undefined` | Validity depends on a known undefined schema result. |
| `NoVerdict` / `no-verdict` | The implementation cannot establish a result, for example because of unavailable content, unsupported capability, cancellation or exhausted work. |

Ready does not mean decidable: each instance may still yield any of these four
results. These results do not establish surrounding document conformance.
`PreparationFailure` is separate from per-value `NoVerdict`; inability alone never
establishes undefinedness. See the [evaluator guide](../crates/openbindings-json-schema-evaluator/README.md)
for current capabilities and budgets.

Core's evaluator seam uses an associated validator type. It does not impose
`Send`, `Sync`, dynamic dispatch or a particular ownership strategy. Prepared
schemas own their validators and inherit their applicable traits. Cloning is
available when the validator is cloneable. The default evaluator shares its
compiled state; each new preparation is independent. Core owns original schema
inventory and resolution through `SchemaEnvironment`; vendor projection is private
to the evaluator. Explicit `ExternalSchemas` supplies immutable resources without I/O.
Changing resources requires a new preparation.

Rust `ResourceIdentity` supports `Eq` and `Hash` using the retained environment
allocation and resource index. Equal identities hash equally; matching URI text
in two environments still gives distinct map or set keys.

`references_with_resources` inspects document references and transitively reached
supplied resources within its existing traversal limits. A complete report can
contain unresolved entries. Missing or malformed `openbindings` declarations use
0.2's schema positions and resolution rules, matching validation of a text that
declares no version. This assumption adds no limitation; a complete traversal is
complete. Well-formed unsupported versions and duplicate member names still
produce empty, incomplete reports. Each `Reference` retains the original keyword
location, spelling and static resolution result. Its `resource_root` (TypeScript
`resourceRoot`) is the original location of the schema beginning the site's
resource, with `None`/`null` exactly for the document resource. A supplied source's
top resource uses that source's URI and an empty pointer, even without `$id`.
This is a location, not the resolved base; conformant Rust callers can use
`SchemaNode::resource_base` for the base.

Inspection has no separate schema nesting bound beyond document admission. The
caller-selected allowance, 8 MiB by default, covers all generated and source text retained in reference
entries: location, resource-root, target and refusal pointers, every origin's
retrieval URI, keywords, spellings, refusal codes/messages and unavailable-resource
URIs. Text uses UTF-8 bytes; pointers through an unpaired name use two bytes per
UTF-16 unit. This bounds text, not total heap usage. Each whole entry is charged
before its text is materialized or cloned. The first entry that does not fit
stops traversal with `ReferenceReport::Incomplete` and a `LimitExceeded` limitation (TypeScript
`limit-exceeded`) with code `reference-report-bytes` and no omitted location. For
unavailable resources, the refusal's `resource_uri` (TypeScript `resourceUri`)
gives the resolved missing identity. Preparation can still succeed; evaluation
reports `NoVerdict(ResourceUnavailable)` when it cannot decide without that
content, with the original reference location when known.

## Editing, admission and diagnostics

Author with exact JSON and path edits. Set a whole namespace if its parent does not
exist; editing does not fabricate containers. Rust `Edit::set(&["sources", key], value)`
owns decoded path segments and the value; `set_at` accepts an RFC 6901 pointer.
TypeScript rows contain only `op`, `pointer` and, for set, `value`. No detached
part-merging or field-collision convention changes the supplied JSON.

Default exact parsing admits 32 MiB UTF-8, 1,000,000 nodes and 10,000 container
levels. Direct whole-document validation uses the same defaults; past a limit it
is undetermined with `byte-limit`, `depth-limit` or `node-limit`. Explicit
`JsonLimits` apply to parsing and are retained for document edits. See
[Reading errors](rust-first-use.md#reading-errors) for diagnostic formatting and
explicit access to retained values.
TypeScript ordinary JSON admission rejects nonfinite numbers, undefined, cycles,
accessors, sparse arrays and unsupported object forms without running getters or
`toJSON`; use exact parsing for original number tokens and raised admission limits.
`ExactJson.from` keeps standard `TypeError` and `RangeError` classes and fixed
messages, with stable `.code` properties. Shape failures use the existing
ordinary admission vocabulary. Ordinary and exact-text admission use the same
`byte-limit`, `node-limit` and `depth-limit` codes, including aggregate limits
reached with nested exact owners. `input-limit` remains only for a bridge
admission limit without an identified cause. TypeScript ordinary conversion
permits 10,000 nested containers and refuses 10,001; Rust's ordinary Serde
conversion permits 128 and refuses 129. Those conversion depths are unchanged.
Caller exceptions remain unchanged. The definitions describe each separate
encoding and evaluation budget.

```ts
export type JsonKind = "null" | "boolean" | "number" | "string" | "array" | "object";
```

`ExactJson.metadata.kind` is `JsonKind`. Schema type assertions can also name
`integer`; their type names are a different vocabulary.

`EditError::code()` / `EditError.code` identifies a failed transaction, including
invalid paths, missing parents, duplicate members, overlaps and admission limits.
TypeScript `inputPointer` is a logical path into refused caller JSON, not invented
source coordinates. It is null when unavailable, including a final document limit.

Findings name original source occurrences. `SourceLocation` has a zero-based UTF-8
byte offset and one-based line and byte column. Equal pointers at duplicate names
can have different offsets. `Pointer` preserves exact UTF-16 segments; quote it for
safe display and convert byte columns against original text for editor coordinates.
Rust serialization replaces each unpaired pointer unit with U+FFFD, so diagnostic
records work with `serde_json::to_value`, `json!`, `to_string` and `to_writer`.
Stored pointers keep their exact units for locating and editing. TypeScript's
bridge preserves exact UTF-16 pointers in JavaScript strings.
`Pointer::to_exact_json(&self) -> String` returns a complete JSON string token
with unpaired units escaped as `\uXXXX`. This exact export preserves the target
and does not alter ordinary Serde. Recover the token through `JsonValue::parse`,
read its string's `utf16_units()`, and call `Pointer::from_utf16`. Use sufficient
admission limits for large tokens. In TypeScript use `JSON.stringify(pointer)`
and `JSON.parse(text)`. Keep the token as JSON text: an ordinary Rust JSON string
value cannot carry an unpaired unit.

Value problems have an instance pointer relative to the evaluated value and a
`SchemaOrigin` with `retrieval_uri` (TypeScript `retrievalUri`) plus an original schema
pointer. Null retrieval URI means the document. `JsonValue::locate`,
`ConformantDocument::locate` and `ExternalSchemas::locate` compute source positions
when requested; TypeScript offers corresponding `locate()` methods with return
type `SourceLocation | undefined`. An unavailable source or node returns
`undefined`. No synthetic
schema coordinate is substituted for an unavailable origin.

Cheap failures contain no problems. An explanation may be incomplete because of
diagnostic policy, work limits or cancellation; inspect `problems_complete`
(TypeScript `problemsComplete`). Schema facts in `details` may contain application secrets. `Debug` is redacted,
and messages stay fixed SDK text. Serializing whole problems into a public
response sends schema facts. An app with a private contract should forward only
`instance` and `code`. Rejected instance values are never copied into facts or
messages.

The default evaluator explains only after establishing failure. It uses the
remaining evaluation work and storage allowance to retain up to 100 distinct
assertions (`MAX_EXPLANATION_PROBLEMS` internally). Problems sort lexically by
exact UTF-16 instance pointer, then schema retrieval URI and exact schema pointer.
Repeated applications of the same assertion to the same occurrence are one
problem. Each missing required member is a separate problem at the existing
parent instance. Problems tied on instance and schema location retain the
schema's `required` array order.

`details` contains established schema facts: `Required { member }` for missing
members (also in active `dependentRequired` and array `dependencies`),
`Type { expected }` for all expected types, `NumericBound { bound }` for
`minimum`, `maximum`, `exclusiveMinimum` and `exclusiveMaximum`,
`SizeBound { bound }` for `minLength`, `maxLength`, `minItems`, `maxItems`,
`minProperties` and `maxProperties`, and `Enum { choices }` for the complete
allowed list. Bounds and enum choices retain original JSON token text without
floating-point conversion. Serialization uses a kebab-case `kind` tag, such as
`{"kind":"numeric-bound","bound":"1E+400"}`.

Keywords without a matching variant, including `const`, `pattern`, `multipleOf`
and `additionalProperties`, have no `details`. Pattern text stays out. Facts
count against the existing diagnostic storage allowance. A fact that cannot
fit or cannot be represented exactly, such as a required member containing an
unpaired surrogate, is omitted and sets `problems_complete` to false. Enum lists
are never partial. These omissions never change the verdict.

Conjunctions, references, properties, items, and active dependencies report all
their applicable failures. A concrete `if` selects only the failing `then` or
`else` branch. Failed `anyOf`, `oneOf`, and `not` each report one problem at the
applicator itself: respectively no alternative satisfies, the match count is
not exactly one, or the negated schema satisfies. Their branch failures are not
separate problems. Likewise `contains` reports its failed match-count assertion
at the array, and `propertyNames` reports at the object because a member name
is not an addressable instance value. An applied Boolean false schema, including
`additionalProperties: false` and `unevaluatedProperties: false`, uses
`falseSchema` at that original schema and the rejected member's instance pointer.

`problems_complete` means every failure under these reporting rules was retained.
It is false when a problem cap, work/storage limit, cancellation, unavailable
location, or unresolved applicability prevents that claim. A fully established
applicator failure can be complete even if an irrelevant branch is unavailable.
Passing values and cheap checks perform no explanation work.

Cancellation is cooperative. A same-thread `AbortSignal` cannot run while synchronous
Wasm occupies that thread. HTTP acquisition has separate asynchronous cancellation,
redirect and deadline policy; see [HTTP discovery](../crates/openbindings-http-discovery/README.md).
Source/kind data and binding selection remain outside core document interpretation.

See [maintainer verification](maintainers.md) for the complete gate sequence.

Parse failures expose `InputError::line()` and `byte_column()` in Rust and
`InputFailure.error.line` and `byteColumn` in TypeScript. Lines and UTF-8 byte
columns are one-based. They are computed once on failure; nothing changes on success.

Invocation preflight returns `Option<ContextChallenge>`: `None` means no prediction,
not guaranteed success. `Invocation` is `#[must_use]` because an unused lazy owner
runs no provider or validation work. Explicit `drop(invocation)` abandons it
intentionally. `InvocationError::binding_not_found()` constructs the
binding interface's missing-binding error. Invocation carriers decode through
`from_json(&JsonValue)`; retain a borrowed frame with `to_owned()` first, a
reference-count clone. Exact object carriers expose `value()` for borrowing;
clone it explicitly when ownership is needed. Carrier `to_json` means bounded
encoding: `InvocationError`, `BindingRequest` and `OperationRequest` retain those
encoders. Schema comparison's `identical` returns `Result<bool,
ComparisonFailure>`; a failure is separate from a negative comparison.
`ComparisonFailure` is a struct with `kind() -> ComparisonFailureKind` (Copy),
`origin() -> Option<&SchemaOrigin>` and `side() -> Option<ComparisonSide>`.
Locations preserve original source frames and are redacted in formatting.
Identity sides are `Left`/`Right`, directional sides `Target`/`Candidate`, and
interface sides `Required`/`Provided` regardless of role. Global limits and
cancellation have neither origin nor side. Use `kind()` for classification;
whole-error equality includes provenance.
`compare` and `compare_with_control` take `Direction`, a present target and a
present candidate, and return `Comparison` with `verdict()`, `reason()` and
`finding()` accessors. `ComparisonContext::new(root)` retains a reusable document;
`with_base` and `with_resource` configure it, and `schema(exact_value)` selects a
`SchemaInput`. Membership uses node identity, not JSON equality: an equal but
separately parsed schema gives a detached input with its own root's assigned
dialect and private call-local declarations. These do not override configured
declarations: a reached URI or anchor claimed by distinct occurrences fails with
`InvalidReference`, even when their JSON content is identical.
Pointer references still start at
the configured document unless a local `$id` changes scope.
As SDK policy, roots with both `openbindings` and `operations` members are
recognized as OBI documents without validating those values.
`SchemaInput::new` remains a convenience for single comparisons;
`with_base` sets the initial base of a standalone schema root and is ignored for
an OBI root. Its document resource cannot be named by a caller URI. Followed
references must reach schema positions; pointers into instance or carrier data
fail with `MissingReference` in every comparison mode.
`context()` exposes its retained context. Indexing is lazy, shared within each
call and reusable across calls. Failed and cancelled work leaves contexts usable.
Rust `ComparisonContext::retained_bytes(&self) -> usize` estimates current source
arenas, configuration and completed indexes on demand. Clones share the estimate;
summing owners overcounts shared allocations. Later lookups can grow it. This
excludes allocator overhead and Wasm memory growth and is neither resident memory
nor the bytes dropping one handle frees.
`check_interface` and `check_interface_with_control` take required/provided
conformant documents, their respective `&ComparisonContext`s, and a callback
`&dyn Fn(&Operation, Option<&Operation>) -> Option<OperationRole>` receiving the
required operation and each distinct provided match, or `None` when missing.
Each context must retain the exact root of its paired proof; an equal but
separately parsed root fails with `ContextMismatch` before callbacks. Initial
cancellation takes precedence. External resources come from the caller and use
their supplied URIs as bases, including for relative references between resources.
The callback selects realized, consumed, both, or excluded for that match. Every distinct
provided match is retained, independent of alias order, with output then input
results including failures. `operations()`, `issues()` and each issue's
`correspondence()` expose keys, role and slot results.
`OperationComparison::required()` returns `&str`, and `provided()` returns
`Option<&str>`. `role()` returns
`Some(Realized)` or `Some(Consumed)` for present matches, never `Both`; an included
missing operation has one correspondence with `None` role. Consumption reverses
input and output flow. Allowance exhaustion remains in slots after setup;
observed cancellation returns an error of kind `Cancelled`. Summary failure policy is
the caller's; only `Verdict::collapse` supplies profile dominance over verdicts.
`identical_with_references` and `identical_with_references_and_control` take a
caller accounting predicate. `ReferenceContext::target()` exposes the initial
resolved target or failure alongside spelling, original schema, resource and base.
Repeating with `&|_| true` asks whether drift lies only behind unaccounted targets.
All controlled calls share one shared `ComparisonLimits` work and memory policy per call.
Public diagnostic formatting omits reasons and document values.

`DefaultOperationInvoker::select(&interface, &address)` and
`PreparedOperation::select(&provider, &choice)` return a core `Binding` using only
the document, address or choice, and `InvocationSupport::can_invoke`. Read its
source, load that source's context, then invoke or preflight the selected binding
explicitly with that context. Prepared selection also checks operation membership.
Selection does not admit the address: an empty ordered list or an empty key in
that list returns `ERR_BINDING_NOT_FOUND`; invocation and preflight return a
`BoundaryFailure` for those malformed lists.

`PreparedOperation::input()` and `output()` borrow its prepared schemas, with
`None` for an undeclared direction. They allow inline value checks before context
loading and invocation without preparing a direction again.

### Context admission

The invocation, kind-description and source-pull crates re-export `Context`,
`ContextChallenge`, `ContextShapeError` and `ChallengeShapeError` together.

`Context::new(JsonObject)` and `Context::try_from(JsonValue)` are fallible and
return `ContextShapeError::{ExpectedObject, DuplicateMember}`. Members remain
opaque. Each request decoder that admits a context member applies the same
check and reports it through its own error: `BindingRequest::from_json` and
`OperationRequest::from_json` return `BoundaryFailureKind::DuplicateMember` at
`/context`, `PullRequest::from_json` returns `RequestError::Shape("duplicate-member")`
and `CheckSourceRequest::from_json` returns `CarrierErrorKind::DuplicateMember`. The
TypeScript facade carries no context, so it has no context admission path.

Context admission is SDK policy: every object in context, including objects
inside arrays, must have unique decoded member names. The pinned context
document is silent on duplicates. This follows the parser-differential reason
in [OBI-01](https://github.com/openbindings/spec/blob/c1419c9fc3ae5a00c2b83da710c11bf67dab7934/openbindings.md#10-conformance):
the same bytes can yield different values across parsers. The SDK refuses
ambiguity without interpreting members, using the exact JSON's existing index.

## Pattern search work limits

Rust evaluator `Limits::pattern_search_work_units: Option<usize>` and TypeScript
`EvaluatorLimits.patternSearchWorkUnits: number | null` bound the SDK work charged
to one uncancellable Original pattern search. The default is 250,000,000;
TypeScript reads it from Wasm through `evaluatorLimits()`. Numeric TypeScript
values use the same nonnegative integer range as other evaluator limits,
0 through 4,294,967,295. `None` / `null` removes the ceiling.

A charge above the ceiling returns `NoVerdict` / `no-verdict`, reason
`LimitExceeded` / `limit-exceeded`, code `pattern-search-work-limit`, before
an oversized search. The charge bounds the actual matcher's state and transition
work over the positions it can inspect. Finite anchored searches need not inspect
the whole input. An unanchored search may prove a match in a bounded prefix; an
inconclusive prefix returns the same limit code. Assertions still see the complete
input. The charge contains no document values.
`evaluation_sdk_work_units` / `evaluationSdkWorkUnits` remains the separate
optional cumulative work cap. Internal fixed-width accounting cannot invent an
uncapped work-limit refusal by overflowing on Wasm.

Changing the search ceiling from its default selects Original, as other changed
evaluator limits do. Explicitly supplying the default preserves lane selection.
The compiled lane retains its separate admission bounds. Explanation preserves
that lane's established verdict if Original diagnostics exhaust their allowance,
returning an incomplete problem list for failure. Neither policy is a
deadline; cancellation is observed around an engine call, never inside it.

## TypeScript preparation and registration limits

```ts
export interface PreparationLimits {
  maxSchemaNodes: number;
  maxIdentityBytes: number;
  maxSdkWorkUnits: number | null;
  maxSdkReservationBytes: number;
}
export interface ResourceRegistrationOptions extends WorkOptions {
  limits?: Partial<PreparationLimits>;
}
export declare function preparationLimits(): Readonly<PreparationLimits>;
export interface PrepareOptions extends WorkOptions {
  resources?: ExternalSchemas;
  limits?: Partial<EvaluatorLimits>;
  preparationLimits?: Partial<PreparationLimits>;
}
```

The emitted `ExternalSchemas` class includes these signatures:

```ts
constructor(entries?: Iterable<readonly [uri: string, document: ExactJson]>, options?: ResourceRegistrationOptions);
with(uri: string, document: ExactJson, options?: ResourceRegistrationOptions): ExternalSchemas;
```

Defaults remain 100,000 schema positions, 8,388,608 identity bytes, `null` work
and 536,870,912 reservation bytes. Partial overrides keep the remaining defaults;
only work accepts `null`, and numbers must be nonnegative 32-bit integers.
`Schema.prepare` and `ExternalSchemas.prepare` take `preparationLimits`
separately from evaluator `limits`. Registration options apply only to that
construction or `with` call, not to later preparation. Limits and cancellation
produce the same Rust refusals: preparation returns `refused`, while registration
throws `SdkError` with code `resource`. Registration limit refusals also carry
frozen `details: { resource: string }`, using preparation's resource strings:
`schema-nodes`, `schema-identity-bytes`, `preparation-sdk-work-units`, or
`preparation-sdk-reservation-bytes`. The message stays
`external schema registration exceeded a preparation limit`. A refused attempt leaves existing owners
usable. SDK reservation estimates are not a Wasm linear-memory ceiling.
