# Current 0.2 alignment development loop

The working branch is `codex/sdk-02-foundation`. The initial foundation checkpoint
is committed and pushed. Continue the migration on this branch; existing APIs have
no compatibility protection. Package publication and integration into main remain
separate from feature-branch development.

The normative target is specification
`c1419c9fc3ae5a00c2b83da710c11bf67dab7934` and adopted interfaces
`3e3dc453a93b611a8e9ea1813ec63a2216830f45`. A newer remote revision does not silently
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

## Status and evidence

Use [current integration guidance](migration.md) for the supported responsibilities
and the [alignment checkpoint records](https://github.com/openbindings/project/tree/main/records/history/sdk-alignment-checkpoint-2026-11-08) for dated evidence.
Qualification of a candidate requires its own source, commands and results;
previous checkpoint totals do not qualify a later candidate.
