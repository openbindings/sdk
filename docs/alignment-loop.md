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

The [current saved checkpoint](alignment-checkpoint.md) records exact completed
slices, unfinished code, remaining gates and where to resume. It supersedes the
older incremental statuses below.

| Stage | Status and next gate |
| --- | --- |
| Inputs, adjudication and design | Pinned and reviewed; preserve current authorities |
| Document/schema foundation | Canonical Rust/TypeScript model and major evaluator semantics implemented; ordinary compiler allocation gate remains open |
| Schema identity | Reviewed and pushed at 42cd6f0; bounded identity only, no directional claim |
| Context and kind description | Reviewed and pushed; current exact carriers and separate query/check providers |
| Invocation | Current interfaces implemented; remaining consumers and final integration checks remain |
| Source pulling | Carriers, requests, relationships, prospective conformance and lazy clients implemented; independent review and legacy consumer retirement remain |
| Consumers and hosts | Finish remaining migration, full workspace/host/CI checks and matched performance |
| Final review | Audit the complete specification-to-code map and all required gates |

## Decisions and unattended work

No decision currently requires Matthew. When one does, record its question, evidence,
affected dependency and safe unblocked alternatives here. Continue every independent
path without assuming an answer. Stop the overall work only when complete or when
no meaningful authorized work remains without that answer or an external change.

The remaining work is implementation and qualification, not permission to change old
APIs. Keep the canonical specification terminology and conceptual organization visible
in the code even where Rust needs additional internal mechanisms.

## Evidence

Historical incremental evidence is retained in the coordinator receipts; it is not
current completion status. Use [migration status](migration-0.2.md), the
[current checkpoint](alignment-checkpoint.md), and package-specific guides for
implemented claims. The final all-workspace/host/performance audit is still open.
