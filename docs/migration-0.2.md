# Current 0.2 alignment checkpoint

This unpublished migration targets specification
`c1419c9fc3ae5a00c2b83da710c11bf67dab7934` and interfaces
`5815fb106bddd2c85ff4e93e0077570b3f6b9e74` on
`codex/sdk-02-foundation`. Existing APIs and module boundaries remain replaceable.
There is no compatibility requirement for the earlier SDK.

## Implemented responsibilities

Core preserves exact JSON, validates the current structural schema and OBI-01
through OBI-14, and retains immutable document proofs. Sources and bindings use
`data`, with omission distinct from explicit null. Schema positions, contained
resource comparisons, duplicate identities, anchors and original reference
queries follow the pinned specification. Fixed contained inventories do not
store an identical dialect profile on every node.

Edits return new snapshots. `ConformantDocument` owns a concrete proof, and
contextual `Schema` values retain the original document after caller handles are
dropped. `ExternalSchemas` and `SchemaEnvironment` preserve original sources,
identities and typed resolution failures. They do not fetch missing content or
expose generated compilation identities as author-written locations.

The optional evaluator implements the generic owned-validator interface. Its
semantic path reasons over original schemas, exact values and coupled evaluation
annotations. Preparation uses one bounded attempt across environment assembly,
custom-profile selection, original meta-schema admission and semantic analysis.
Standard draft-07, 2019-09 and 2020-12 external resources and supported custom
2020-12 vocabulary combinations are implemented. Custom admission validates the
consumer against the selected meta-schema's actual restrictions and references.
Required format assertions support all 19 standard attributes; optional format
assertion remains annotation-only. See [evaluator contracts](schema-evaluators.md)
for the interpretation policies and remaining allocation gate.

`openbindings-context` and `openbindings-invocation` implement the adopted current
interface carriers. Invocation has owned original-document requests, explicit
provider/evaluator/resource dependencies, separate invocation and preflight,
shared context and consuming prepared validators. Consumer migration is ongoing;
the obsolete source-inspection and interface-synthesis companions remain to be
replaced by the designed current interfaces.

## Qualification

The current document/evaluator slice passes the 505-case production corpus,
1,546 required evaluator values and the retained admission negatives. The 2,348
auxiliary reference inputs retain their independently reviewed current obligations
and concrete-completion controls. These overlapping inventories are not additive
coverage. Native and Rust 1.88 core/internal-schema/evaluator suites pass 502
checks, and Rust 1.88 also builds the WebAssembly bridge.

Independent reviews cover custom profile selection, original meta-schema
admission, aggregate preparation budgets and the compact inventory layout.
Semantic primitive/format review and current host/package results are recorded in
the migration receipts. Finite passing controls do not establish complete JSON
Schema capability or completion of the SDK migration.

## Remaining work

- Resolve the two harder recursive Undefined obligations and adjudicate the
  remaining recursive annotation control.
- Finish ordinary user-schema backend allocation qualification.
- Replace parallel authoring/plain/conformant representations with canonical
  owned domain views; complete the planned module and terminology organization.
- Implement schema comparison, kind description and source pulling against the
  pinned current interfaces, and finish invocation consumers and examples.
- Complete TypeScript organization, all-host/package qualification, CI and the
  final matched-toolchain performance campaign. No performance threshold is waived.

The [development loop](alignment-loop.md) continues through implementation,
validation, independent review, corrections and feature-branch checkpoints.
The normative inputs and full alignment map are retained in the adjacent
`sdk-migration` coordination repository. No release, merge or completed-alignment
claim is implied by this checkpoint.
