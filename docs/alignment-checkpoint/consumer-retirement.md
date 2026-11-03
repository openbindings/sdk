# Current interface consumer migration and legacy retirement

Current target remains core c1419c9 and interfaces 5815fb1. No compatibility
obligation preserves inspector/synthesizer models, shared legacy support verdicts,
detached Source/Operation records, multi-source synthesis requests, targets,
coverage envelopes or invocation without an original document/selected binding.
Matthew reaffirmed that none of these old APIs needs preservation. An old test is
retained only when it establishes a current specification/interface obligation or
an actual consumer requirement. Historical behavior, exports, error codes and test
counts are not compatibility requirements. Reexpress useful jobs through the new
model and delete obsolete assertions; do not build adapters to keep old code alive.

## Replacement sequence

1. Qualify `examples/source-pull-consumer` against source dependencies, extracted
   Cargo packages and a real local async Wasm/TypeScript bridge. Its two kinds
   project from source alone, then use current schema identity and reports.
2. Reexpress remaining catalog dispositions, malformed inputs, attribution,
   retained owners and borrower/provider-state jobs. Do not preserve old failure
   codes or the old conflation of unread source with unrepresented offered items.
3. Migrate the binding/operation invocation consumer to original-document requests,
   explicit adopted additions, current lifecycle/error/context contracts and
   separate invoke/preflight. Keep all meaningful interaction/value/effect checks.
4. Port the authoring and invocation performance workloads with explicit equivalence
   notes. Keep original timing/memory gates and matched toolchains; a resource
   refusal cannot pass a workload requiring a complete result. The old authoring
   benchmark includes 0/100/1,000/10,000-item catalogs; qualify the new checked path
   at those scales rather than silently dropping the largest lane.
5. Remove obsolete crates/support records, fixtures and verifier scripts. Update
   aggregate verification, dependency isolation, package-candidate, consumer feature
   combinations, documentation and API inventory. Run the complete resulting graph.

## Behavior adjudication

| Historical consumer job | Current witness or required translation |
| --- | --- |
| Inventory then author a document | Explicit pull report inspection, caller-selected adoption, edited-document conformance |
| Partial inventory / unknown remainder | `exhaustive:false` with limitation; preserve observed additions and existing coverage |
| Known implementation gaps | `omitted:implementation-unsupported`; completeness differs independently from whether the whole source was read |
| Empty / kind-excluded / malformed | Complete empty report, excluded omission with rule, or native operational inability according to kind semantics |
| Loose schema framing hints | Source-owned projection and disclosed loss; never adapt offered contracts to destination schemas |
| New contract versus existing operation | Profile identity of both optional slots; conflict on difference, operational inability on failed comparison |
| Current versus stale existing entry | Kind's exact would-write comparison, including annotations; profile identity is not the stale test |
| Removal-only drift | Native operational inability or separate caller-owned removal, with no delete sentinel |
| Invalid emitted document / repair evidence | Local prospective conformance failure, or caller edit validation; never hand off an invalid result as checked success |
| Retained exact values after owner drop | Original proof and exact result children survive request/provider/client scope; numbers and absent/null remain distinct |
| Unicode source identities | Retain kind-owned identity in data; project legal canonical core keys rather than inserting invalid Unicode keys |
| Same literal id in different kind-defined scopes | Preserve the kind's complete selector and original selected source; no generic dependency/target namespace assumptions |
| Shared, borrowed, non-Clone or local Rc providers | Current GAT futures and shared cancellation preserve native ownership without imposing Send/Sync universally |
| Real pending work and cancellation | Actual provider suspension, scoped child cancellation, cleanup ordering, no caller cancellation on drop |
| Global supported-kind list/verdict shared by all roles | Retire; kind-description claims support per operation, while operation-specific failures remain separate |
| Multi-source synthesis attribution | Explicit single-source requests against retained original documents, with distinct source keys and caller composition |
| Byte-identical synthesized document ordering | Retire incidental emitter snapshots; preserve exact authored values and required semantic ordering |
| Strict synthesis failure containing partial coverage | Retire envelope; current report retains partiality, caller decides whether to adopt |
| Inspect target then invoke without authored keys | Retire API assumption; explicitly adopt a binding and invoke it through its original document |
| User schemas, aliases and explicit binding selection | Preserve in the current operation consumer, including early invalid selection and withheld rejected values |

The first external fixture copies the already independently reviewed finite kind
witnesses into an independently compiled package and adds real suspension and
ownership checks. It does not yet retire all historical catalog jobs above, nor
claim to be a complete implementation for arbitrary data under its example kinds.
The crate-local witnesses remain until relocation and external qualification are
recorded, so no existing evidence is removed merely to obtain a green build.
