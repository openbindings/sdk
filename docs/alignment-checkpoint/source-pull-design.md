# Source-pull implementation boundary

Authority is the current pinned `interfaces/source-puller/{README.md,0.1.json}`
at 5815fb1 and core at c1419c9. The approved architecture replaces retired
inspection/synthesis with `openbindings-source-pull`. Existing target/coverage
models do not constrain this crate. The pinned `conformance/pull-report` corpus contains 21 cases (14 structural
positives and 7 negatives), including completePull expectations and application-owned
report fields. Preserve these unchanged, all five published operation example pairs,
and independent positive/negative relationship controls from prose/schema.

## Public ownership and modules

`PullRequest` retains an original ConformantDocument, exact selected source key
and optional shared Context. The checked client requires that source to exist
before provider dispatch. A provider owns borrowed associated futures and an
associated native Error; operational inability must not be fabricated as a kind
rejection. ContextRequired remains the shared challenge. Query failure codes, if
exposed as exact records, are SDK representation policy, not a new universal wire
error object or invocation lifecycle. Cancellation uses the existing attempt token.

`PullResult` owns the exact `{additions,report}` object. Owned report/addition/entry
handles retain its arena and expose published vocabulary, not reconstructed data.
ExistingStatus, Change and OmittedStatus represent published finite string sets.
BindingProposal and SourceProposal expose only their declared authority fields;
unknown output members stay retained as carrier data without gaining edit power.
Schema proposals are exact Boolean/object values. No checked carrier claims
schema validity, future availability, faithful execution or permission.

Modules separate carriers/report access, request ownership, structural result
relationships, prospective document editing/conformance, provider/client work,
and errors. Source projection is an ordinary kind-owned function; there is no
required public intermediate target or projection model. The optional comparison
companion supports kind implementations' reuse decisions but core does not depend
on pulling, and the generic client cannot reconstruct offered schemas from opaque
binding data. No TypeScript role API is implied.

## Carrier and client validation

Carrier constructors enforce the current field shapes and prose relationships,
including one existing-entry target, stale-only proposal/change, nonempty proposals,
no explanation fields on current entries, a required rule on excluded omissions,
a limitation when non-exhaustive,
true-only reused, distinct uses labels and core binding/operation fields where
those document entries are inserted. Preserve open output fields, exact numbers
and UTF-16, absent data versus null, and separate runtime context.

The client checks finite request/response/entry limits and cancellation. It checks
new map keys and the operation key/alias namespace; bindings must name the selected
source. Every added binding has exactly one matching added report entry. Reused
means the referenced operation already exists and is not an added operation;
ordinary additions reference added operations. Existing reports cover each input
binding of this source exactly once, cannot name another source's binding, and
name existing schemas or the selected source only. Uses labels name actual additions;
self labels are not other additions. Known report-to-report dependencies can be
checked, while closure within opaque kind data remains a producer claim requiring
kind witnesses. No generic field-name search pretends to discover those dependencies.

Create a prospective immutable document containing all additions and require core
conformance under explicit limits before handing off the result. That establishes
only document relationships, not the truth of the producer's reuse/fidelity/closure
claims. The request and original source remain unchanged. Source operations are
never invoked by the client. Reading/discarding is inspection. Applying selected
additions or stale proposals is a caller edit, not an implicit effect of pulling.

`PullReport::is_complete` is exactly exhaustive and no implementation-unsupported
omission. Loss, invalid items, kind exclusions and conflicts do not independently
make it incomplete. It is a condition on the report, not proof the source was read.

## Implementation and evidence sequence

1. Pin all 21 pull-report cases, five published example pairs and carrier definitions; implement exact report,
   proposal and additions carriers plus original-document requests.
2. Add relationship checks and prospective document validation, with negative
   collisions/aliases/attribution/report/proposal/uses witnesses before clients.
3. Implement explicit lazy SourcePuller/PullClient with scoped cancellation,
   separate operational/challenge failures and finite handoff policy.
4. Two independent illustrative kinds exercise no-data and embedded-data sources;
   kind-owned projection sees source alone, then compares offered/current contracts
   via qualified profile identity. Exercise new/reused/conflicting/stale/unmatched,
   partial and complete reports; preserve source data/context and author-owned fields.
5. Caller examples inspect/discard, adopt a selected dependency-closed subset,
   and explicitly apply a stale proposal. Revalidate edited documents without
   claiming live-state or unchanged-fidelity proof.
6. Independent review, Rust 1.88/current, Wasm portability, dependency isolation,
   docs and cancellation/resource checks precede retiring old companions and
   migrating their remaining consumers/benchmarks. Preserve genuine behavior
   witnesses; obsolete target/synthesis APIs receive no compatibility shim.

No user decision is required to begin this specified work. The comparison identity
slice's documented capability refusals remain failures of a reuse decision, never
permission to claim reused or to silently rewrite offered/existing schemas.

## Open carriers and restricted proposal authority

Independent review distinguishes metadata retention from authoring power. Binding
proposals require at least one of data/input/output; uses alone is insufficient.
Source proposals require data, including explicit null; schema proposals may be
false, true or an empty object. Unknown output metadata may survive losslessly,
but a proposal must reject known fields outside its authority: author-owned
operation/binding fields, rebinding operation/source, aliases, or source
kind/description. Generic proposal application copies only its permitted fields,
never all retained members. Core source/binding entry field rules remain separate
from the report/proposal carrier's open metadata boundary.


## Proposal absence is not removal

The published proposal cannot encode deletion of a present data/input/output field.
Omission proposes no change, data:null proposes JSON null, and input/output:null is
invalid. A removal-only stale observation has no truthful representable required
proposal. Producers must report operational inability rather than fabricate current
or unmatched status, hide it solely in omitted, or introduce a delete sentinel.
A caller may perform a separate explicit document removal edit outside this
interface. This publication boundary is not a reason to stop the bounded companion.
