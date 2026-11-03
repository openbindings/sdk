# Independent source-pull implementation review

**Bounded PASS at `final-source-hashes.json`.** No production finding remains open. One P2 correctness defect in the illustrative kind reconciliation was reproduced and corrected; its initial evidence is retained. Production source was read only. This receipt qualifies the source-pull request/client, mechanical relationships, prospective immutable-document boundary, and the documented relationship/edit-metadata resource scope. It does not qualify all SDK integration, arbitrary kind implementations, or whole-call allocation/work accounting.

Authority: core c1419c9fc3ae5a00c2b83da710c11bf67dab7934; interfaces 5815fb106bddd2c85ff4e93e0077570b3f6b9e74, especially source-puller README §§Additions, Report/Added, Existing, and What a caller does; its unchanged 0.1.json. The reviewed implementation design and independent 74-control design ledger remain the governing SDK boundary. No historical API compatibility obligation was imposed.

## Finding and correction

**R-SP1 — P2, resolved: operation reuse identity was incorrectly used to decide existing schema drift.** The initial `tests/kinds.rs::reconcile` called `slots`, then emitted input/output proposals only for profile `Identity::Different`. With an existing input `{"type":"string","title":"old"}` and an offered input `{"type":"string","title":"new"}`, the illustrative embedded kind reported `Current`. The profile deliberately ignores annotations, but the kind projected the changed title. The publication's existing-entry table says current means the puller would write the entry unchanged; stale means it would write differently, with the changed fields in a proposal (pinned README lines 142–155). The profile-identity rule applies specifically to adding a binding against an existing operation (lines 70–80, 121–123).

`R-SP1-before.log` records the independent assertion failure (Current versus Stale), `R-SP1-kinds-before.rs` preserves the initial kind helper, and `R-SP1-independent-kind-control.rs` preserves the counterexample. The corrected kind uses exact semantic JSON equality for existing input/output drift, with explicit absent/present handling; removal-only changes remain operational inability. Profile identity remains in added-binding reuse decisions. The unchanged independent title counterexample now passes in `final-tests.log`, as do the new production controls for title drift, object-member order, and equal exact numbers 1/1.0. No generic client source fix was needed, since that client correctly does not infer kind-owned unchanged/fidelity judgments.

## Relationships and ownership

The implementation enforces the mechanical publication relationships without promoting producer claims to proof:

- Original operation, schema, and binding keys are reserved separately; operation aliases additionally reserve the flat operation namespace. Report operation names are canonical keys, never alias lookups. Every added binding names the selected source and receives exactly one matching added report. `reused:true` selects an original operation, ordinary additions a new operation. Prospective core validation handles added aliases and other inserted core fields.
- Existing coverage requires every original binding of the selected source, including partial pulls; foreign-source bindings are refused. Schema reports require an existing schema but do not pretend the generic layer knows which schemas the source offers. Existing source reports must name the selected source. Duplicate targets are compared by role and key.
- Uses labels identify actual additions. The closure rule includes each retained added binding's declared uses and its new operation, with the current entry's own binding/new operation treated as implicit retained members. Each edge's required set is checked against the complete declared set, so cycles are accepted when closed without recursive graph expansion. Binding proposals receive the same declared-dependency closure check but no fabricated implicit new operation. Opaque kind data is not scanned for guessed references.
- Only the three declared additions maps enter the prospective document. Unknown output metadata and proposals remain retained report data. The original proof and source remain unchanged. Original JSON admission limits survive into edits; a low-original-byte-limit probe refuses oversized additions without losing the original proof. A successful all-additions snapshot does not authorize arbitrary subsets, stale proposals, or later live-state claims.

The public carrier boundary remains the separately reviewed one: exact JSON/UTF-16, absence versus null, role-local author-field restrictions, open output metadata, and a closed request. `PullRequest::from_json` admits a core proof before client handoff policy; its documentation correctly asks byte callers to choose JSON limits first. Scalar selected keys are appropriate because conformant document entry keys cannot contain non-scalar strings. Separately supplied opaque context is retained without merging it into the document.

## Client lifecycle and errors

`PullClient::pull_source` performs request/source checks before provider construction, owns one explicit attempt and the original proof, and delegates finite provider ownership to the previously reviewed shared `run_attempt`. Cancellation reaches constructor/poll work, cancels the child before the provider future's destructor, and is checked again after cleanup and local result validation. There is no implicit retry or context collection. Native errors remain typed and owned; challenges have an exact JSON response-byte check; arbitrary native errors are not misrepresented as JSON payloads subject to that bound.

Independent controls cover a local Rc provider/native error, a Send-but-not-Sync Cell-based provider future, all 12 constructor/poll/destructor cancellation × success/operational/challenge/provider-cancel combinations, and every one of 30 local validation cancellation checkpoints followed by a fresh successful retry. The existing shared-helper receipt independently covers constructor panic, synchronous cross-thread cancellation, and scoped relay/waker cleanup; this change does not alter that helper.

## Resource review and measurements

The claim is deliberately limited to handoff sizes, relationship work/indexes, and edit metadata. Already admitted JSON, provider work, core rebuilding, and core conformance internals are excluded. Source comments and README state this, and cancellation is checked immediately before and after the synchronous core stages. No whole-call budget claim is inferred from retaining JsonLimits.

The relationship maps borrow original strings/JSON handles. Their insertion-only BTreeMap reservations cover two maximal nodes per insertion; the supported layouts have ample fixed-node overhead and tuple-size padding. Replacement attempts return immediately, without deletion/reinsertion invalidating the prefix argument. Search work charges key bytes and a conservative logarithmic comparison count. Edit vectors reserve their exact maximum count before insertion. Each owned edit path reserves eight times the original key bytes plus constant overhead, covering JSON Pointer escape expansion and cumulative String growth; a newly absent section uses a constant-sized path. Checked counter arithmetic and sticky refusal were replayed, including a later overflowing vector request after an earlier refusal.

A counting allocator measured the real copied functions. The receipt-only metadata switch returns immediately before the core edit call, after executing the actual relationship and edit assembly code. It defaults false and was enabled only in the serialized allocation test. No accounting or semantic production branch was changed.

| Shape | Relationship cumulative / reserved bytes | Including edit metadata cumulative / reserved bytes |
| --- | ---: | ---: |
| Empty maps, one schema addition | 368 / 1,280 | 496 / 1,504 |
| 1 new binding/operation | 2,752 / 9,344 | 2,952 / 9,760 |
| 100 new bindings/operations | 64,856 / 807,680 | 72,184 / 827,824 |
| 1,000 new bindings/operations | 634,424 / 8,065,280 | 706,552 / 8,272,624 |
| 1 name extended by 100,000 characters | 2,752 / 9,344 | 265,064 / 809,760 |
| 100 names extended by 1,000 characters | 65,576 / 807,680 | 274,504 / 1,627,824 |

Each shape passed zero, 1 KiB, exact, and one-below allocation caps independently for relationships and metadata (48 lanes), with actual requested cumulative bytes never exceeding the selected cap. Exact/one-below work and entry caps also passed. The long-name stress lane uses explicit generous work allowances to examine allocation separately; its 56,619,680 relationship-work charge honestly exceeds the default 20 million. No default or oracle was weakened. The allocator measurements are a native requested-allocation check, not a claim about process RSS, an allocator's overhead, or all Wasm layouts.

## Executed evidence and limits

`final-tests.log` contains **33 passing groups**: 2 budget groups, 8 preserved carrier groups, 4 client groups, 5 relationship groups, 5 corrected illustrative-kind groups, 7 independent implementation groups, the independent kind counterexample, and one allocation group. These include all unchanged 74 design controls (duplicate uses is deliberately refused at the earlier carrier gate), all 21 published pull-report cases, and all five published example pairs. `summary.json` records fixture preservation, counts, instrumentation, and final source identities.

The independent closure set is saved as `closure-controls.json`: **500 independently judged cases, 75 accepted / 425 rejected**. Its oracle computes required retained sets directly from the publication rule, rather than copying SDK outputs. It exercises mixed new/reused operations, cycles, missing transitive operations/uses, and shared schema uses. Additional public controls cover existing proposals, exact decoded names without case aliasing, metadata nonapplication, original admission-policy retention, cancellation, and native ownership.

The two illustrative kinds project from source-only inputs before consulting the destination. They separately exercise no-data and embedded-data kinds, correct kind identifiers, new/reused/conflicting offers, current/stale/unmatched/partial reporting, comparison inability, removal-only inability, explicit subset adoption, explicit proposal edits, repeated pull, and original immutability. They are finite explanatory examples, not qualified implementations for every possible catalog/data shape. The core author-field locations are correct: operation description is retained on the operation; preference and idempotent remain on the binding, notwithstanding the looser source-puller prose wording.

Final source copies are in `production-snapshot`; executable isolated copies are in `probe`. Instrumentation is confined to the receipt and documented in `summary.json`. The only production-source change during this review was moving budget tests to the end of the module; the final version was recopied and replayed. The kind test correction was separately recopied and rerun. Root-reported MSRV/Wasm/clippy/docs gates are not relabeled as independent executions here. Companion retirement and other SDK consumer migration remain outside this PASS.
