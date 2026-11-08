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
| Schema identity comparison | [openbindings-schema-comparison](../crates/openbindings-schema-comparison/src/lib.rs) | `cargo doc --locked --no-deps -p openbindings-schema-comparison` |
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
checked pull validates labels and all additions together. A caller keeping part
of a pull follows schema references and each reached added binding's operation
and `uses`, as the [adapter guide](writing-a-kind.md#what-goes-in-uses) explains.

## Exact documents and conformance

Rust `JsonValue::semantic_eq(&other)` compares exact JSON values without
floating-point conversion. It returns `Some(true)` for `1e400` and `10e399`,
`Some(false)` for unequal values, and `None` if either value contains duplicate
object member names. Token spelling is preserved independently of value equality.

| Operation | Contract |
| --- | --- |
| Rust `Document::parse`, TypeScript `Document.parse` | Admit exact JSON and retain an immutable snapshot, including nonconformant drafts. Syntax, encoding and admission failures are separate from document findings. |
| Rust `Document::new`, TypeScript `Document.create` | Create a draft declaring the supported specification version and an empty operations map. |
| `Document::value` / `Document.value()` | Inspect original JSON, including extension members, original number tokens and duplicate occurrences. TypeScript acquires a disposable owner. |
| `set`, `remove`, `edit` | Return a new immutable snapshot. An atomic batch addresses the original revision and preserves untouched bytes. Failure leaves all earlier owners unchanged. |
| `Document::validate` / `Document.validate()` | Assess all applicable OBI rules. Only established conformance creates a `ConformantDocument`. Unsupported declared versions have a separate refusal. |
| `validate_document` / `validateDocument` | Assess exact source directly, including syntax failures. Rust can return a proved document; TypeScript returns only a detached report. |
| `ConformantDocument::document`, `into_inner` | Borrow the retained draft snapshot, or consume the proof and recover it. |
| `ConformantDocument.asDocument()`, `intoDocument()` | Acquire a separate editable snapshot owner, or consume this proof. Consuming revokes aliases of this handle; independently retained owners survive. |

`ConformantDocument` is privately constructed and has no editing methods. Its domain
reads need no repeated OBI assessment. Raw document inspection provides evidence for
repair without exposing partly interpreted canonical entries. Diagnostic
`references()` is available on both states: it acquires no resources and proves
neither document conformance nor value decidability.

`Evidence` is `Met`, `Violated`, `Inconclusive` or `NotApplicable`. A document's
`Conformance` is conformant, nonconformant or undetermined. Findings explain rule
evidence; omitted findings do not erase an established violation. Use rule IDs,
codes and evidence for logic, and treat explanatory messages as display text.

### Document finding codes

The [complete finding-code table](../crates/openbindings/README.md#document-finding-codes)
lists every stable `Finding.code`, its OBI rule and its meaning. It applies to
Rust and TypeScript conformance reports, including parsing and admission limits.

## Canonical entries

`Operation`, `Source`, `Binding`, `Dependency` and `Example` retain their exact
original declaration and the conformant document that proves it. They are read
owners, not detached authoring structures. Rust clones and TypeScript `retain()`
share immutable storage; dropping a parent does not invalidate its acquired children.

| Read | Meaning |
| --- | --- |
| `operations()` | Operations in lexical primary-key order. |
| `operation(key)` | Exact primary-key lookup. |
| `resolve_operation(name)` / `resolveOperation(name)` | Primary key or declared alias in the proved shared namespace. |
| `bindings()`, `sources()`, `dependencies()` | Optional canonical collections in lexical key order. Absence differs from present-empty. |
| `binding(key)`, `source(key)`, `dependency(key)` | Exact namespace lookup. |
| `schemas()`, `schema(key)` | Contextual named schemas; reading performs no preparation. |
| `Operation::input`, `output` | Optional contextual `Schema`; absent never means a fabricated true schema. |
| `Operation::bindings` | Associated canonical bindings in lexical order, without preference ranking or selection. |
| `Binding::operation`, `source`; `Dependency::operation` | The referenced canonical entry from the same immutable proof snapshot. Separate key getters expose the declared primary keys. |
| `Operation::examples`, `example(key)` | Canonical examples with operation-local keys. Reading does not validate an example instance. |
| Source/binding `data`; example `input`, `output` | Optional exact JSON owners. Absence differs from explicit null, and wide numbers retain their tokens. |

Optional scalar fields remain optional: `None` in Rust, `undefined` in TypeScript.
Absent boolean flags do not become false. Declared preference is an optional
interoperable integer, not an SDK preference type. Kind names compare exactly;
an absent dependency kind filter accepts every kind, while a present filter is
nonempty. Alias and tag arrays preserve declaration order; TypeScript freezes them.
OBI-01 proves scalar strings throughout a conformant document. Unpaired UTF-16
remains representable in raw JSON, findings and external instances, not in a proof
that would violate that rule.

TypeScript collection methods acquire a fresh owner for every returned entry.
Dispose each element, including every schema-map value. Scalar getters acquire no
owner. `value()` and `document()` do acquire owners. Exact traversal cursors and each
yielded member/value also have independent lifetimes. Finalizers are best effort;
disposal is not a promise that process RSS or Wasm capacity shrinks.

## Schemas and value results

A `Schema` owns its original location and conformant context. Rust
`schema.prepare(&evaluator, &external, &limits)` returns
`Result<PreparedSchema<E::Validator>, PreparationFailure>`. TypeScript
`schema.prepare({ resources, limits, signal })` returns a `ready` branch owning a
`PreparedSchema`, or a `refused` branch with a `PreparationFailure`. Preparation
failure judges no instance. A missing operation or absent schema is found before
preparation and is not a preparation failure.

`evaluate` is the cheap value check; `explain` requests diagnostics for the same
semantic result. Rust `evaluate_json` and TypeScript `evaluateJson` first admit exact
JSON text or bytes. TypeScript `evaluate` accepts checked ordinary JSON or an
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

`references_with_resources` inspects document references and transitively reached
supplied resources within its existing traversal limits. A complete report can
contain unresolved entries. Missing or malformed `openbindings` declarations use
0.2's schema positions and resolution rules, matching validation of a text that
declares no version. This assumption adds no limitation; a complete traversal is
complete. Well-formed unsupported versions and duplicate member names still
produce empty, incomplete reports. Each `Reference` retains the original keyword
location, spelling and static resolution result. The report retains its existing
256-schema depth stop and 8 MiB aggregate pointer allowance, including reference,
target and refusal locations. For unavailable resources, the refusal's `resource_uri`
(TypeScript `resourceUri`) gives the resolved missing identity. Preparation can
still succeed; evaluation reports `NoVerdict(ResourceUnavailable)` when it cannot
decide without that content, with the original reference location when known.

## Editing, admission and diagnostics

Author with exact JSON and path edits. Set a whole namespace if its parent does not
exist; editing does not fabricate containers. Rust `Edit::set(&["sources", key], value)`
owns decoded path segments and the value; `set_at` accepts an RFC 6901 pointer.
TypeScript rows contain only `op`, `pointer` and, for set, `value`. No detached
part-merging or field-collision convention changes the supplied JSON.

Default exact parsing admits 32 MiB UTF-8, 1,000,000 nodes and 10,000 container
levels. Direct whole-document validation without explicit limits has no nesting
cap. Explicit `JsonLimits` apply to parsing and are retained for document edits.
TypeScript ordinary JSON admission rejects nonfinite numbers, undefined, cycles,
accessors, sparse arrays and unsupported object forms without running getters or
`toJSON`; use exact parsing for original number tokens and raised admission limits.
The definitions describe each separate encoding and evaluation budget.

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

Value problems have an instance pointer relative to the evaluated value and a
`SchemaOrigin` with `source_uri` (TypeScript `sourceUri`) plus an original schema
pointer. Null source URI means the document. `JsonValue::locate`,
`ConformantDocument::locate` and `ExternalSchemas::locate` compute source positions
when requested; TypeScript offers corresponding `locate()` methods. No synthetic
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
exact UTF-16 instance pointer, then schema source URI and exact schema pointer.
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
