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

The complete host run has 161 passing tests and three open evaluator replay tests:
two still require all historical evaluator documents to be conformant, and one uses
the earlier admission expectations for the auxiliary reference matrix. The reviewed
1546-case evaluator suite and 20 moved negative admission cases are the next inputs
to integrate. These failures remain open gates; the ownership review does not accept
them or qualify the full foundation. Typed draft models, parallel views, and the old
evaluator seam also remain scheduled for replacement. No user decision is pending.
