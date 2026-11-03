# Current 0.2 alignment development loop

The working branch is `codex/sdk-02-foundation`. The initial foundation checkpoint
is committed and pushed. Continue the migration on this branch; existing APIs have
no compatibility protection. Package publication and integration into main remain
separate from feature-branch development.

The normative target is specification
`c1419c9fc3ae5a00c2b83da710c11bf67dab7934` and adopted interfaces
`5815fb106bddd2c85ff4e93e0077570b3f6b9e74`. A newer remote revision does not silently
change the target. Any deliberate re-pin needs its own delta and qualification.

## Repeat for each implementation slice

1. Read the affected published obligations and reviewed API/ownership design.
2. Implement the selected domain boundary and migrate affected consumers, examples,
   documentation and tests. Retire obsolete interfaces explicitly; do not add aliases
   or wrappers merely to preserve the previous SDK.
3. Validate against the independent published cases and adjudicated historical cases.
   Include negative controls and exact JSON, ownership, resource and failure witnesses.
4. Review new failure meanings, reference semantics and state transitions independently.
   Resolve findings before treating the slice as qualified.
5. Run the relevant build, formatting, lint, host and performance gates. A capability
   refusal remains a gap when a case expects a semantic answer. Inconclusive timing is
   not a performance pass. Record the scope and limitations of every result.
6. Commit and push a reviewable checkpoint, update implementation status and evidence,
   and continue directly to the next unblocked slice.

The complete plan, source-to-code map, API sketches, adjudication ledger, frozen input
manifest and baseline receipts currently live in the adjacent sdk-migration repository.
Required qualification inputs and executable checks must also be available in this SDK
repository before CI can establish their gates; a local sibling checkout is not a CI
dependency.

## Implementation sequence

| Stage | Status and next gate |
| --- | --- |
| Inputs, adjudication and design | Complete; independent reviews pass, native baselines frozen |
| Document/schema foundation | Admission slice passes the 505 current core cases; continue immutable documents, concrete proofs, owned domain views, schema environment and evaluator integration |
| Schema identity | Implement profile normalization/identity with contextual resources and exact values |
| Context and kind description | Implement shared challenges and separate descriptor/check provider traits |
| Invocation | Replace old carriers with document/key requests; qualify selection, preflight, value gating and lifecycle |
| Source pulling | Replace inspection/synthesis with additions/report, identity reuse and kind-specific projection hooks |
| Consumers and hosts | Qualify Rust, Wasm, TypeScript, package archives, documentation and full examples |
| Final review | Close semantic/organizational gaps and all required CI/performance gates against the recorded candidate |

## Decisions and unattended work

No decision currently requires Matthew. When one does, record its question, evidence,
affected dependency and safe unblocked alternatives here. Continue every independent
path without assuming an answer. Stop the overall work only when complete or when
no meaningful authorized work remains without that answer or an external change.

The remaining work is implementation and qualification, not permission to change old
APIs. Keep the canonical specification terminology and conceptual organization visible
in the code even where Rust needs additional internal mechanisms.

## Ownership checkpoint

The concrete document proof, conformance module, exact object access, and immutable
Rust/TypeScript edits pass bounded independent review. Core/schema/JSON/bridge tests:
271 passed, zero failed, one preexisting ignored. All 505 pinned core cases pass.
Workspace lint passes with warnings denied. TypeScript type checks and Rust prose
examples pass. The unchanged Wasm memory bounds pass after reference registration
was split into declarations followed by queries.

## Current evaluator and contextual-schema work

The reviewed 1546 value cases and 20 negative admission cases are now repository-local,
with a checked 1566-ID partition and nine input-mutation controls. Capability refusal
no longer substitutes for a required semantic answer. The auxiliary 2348 cases preserve
all original inputs: 194 are current OBI-12 admission negatives, and one historical
Unicode-capability expectation now requires its specified satisfying verdict. The
independent completion oracle checks 1552 complete values and 182 completion groups.

Unicode property matching and safe terminal positive lookahead have passed independent
correctness and allocation review. Exact contained identities and original-inventory
fragment lookup pass independent review after corrections for supplied base spelling
and nested normalized-ID anchor collisions. The strict evaluator suite retains 10 open
external dialect/vocabulary cases. Eight additional external-to-contained reference
cases require semantic undefinedness but currently refuse preparation. Neither group
is a passing semantic gate.

Owned Rust Edit inputs and contextual Schema views are being qualified. Schema views
include named schemas and operation slots and survive document/operation disposal.
The TypeScript Schema facade mirrors that ownership. Typed draft models, parallel
views, the old evaluator seam, and obsolete companion interfaces remain scheduled
for replacement. Full workspace/host and final performance gates remain open.
No user decision is pending.
