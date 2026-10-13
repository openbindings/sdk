# Astra consumer acceptance audit

Reviewer: the independent consumer/effects reviewer in this task. Date: 2026-10-13.

This records my actual bounded audit and subsequent source verification. I used the frozen authenticated-JSON plan, cases and decision ledger, current allowed adapter/generic/client source, and maintained fresh consumer/tests. I did not read other reviewers' reports or old OpenAPI binding drafts or their derivative expectations. The final acceptance audit and its verification were read-only source reviews: I ran no builds, tests or timing commands during the coordinator's qualification/performance runs.

## Initial AJ01–AJ22 acceptance audit

The maintained tests did **not yet establish every literal AJ acceptance assertion**. These were evidence gaps, not newly discovered runtime defects. The most useful additions were concentrated in the shared consumer, which already ran through native and Wasm entry points.

| Case | Evidence at the initial acceptance audit | Remaining literal gap or assessment at that point |
| --- | --- | --- |
| AJ01 | Six-row oracle, manual binding, manual operation, renamed/cold generated operation; exact headers/body/output. | Bearer POST existed as /combined with an API key. Bearer-only POST was not explicitly required by the literal wording, although a small override case could clarify the intended vertical. |
| AJ02 | key-post oracle through all three paths; bearer absence checked. | Covered. |
| AJ03 | Combined POST had exact wire assertions; two-key variant only checked success/count. | Missing one credential or one grant within a conjunction was untested. Add these negatives with zero entries/output, and exact two-key wire checks. |
| AJ04 | Missing selector with one credential; duplicate alternatives; invalid selectors; unselected credential exclusion. | Missing both-credentials/no-selector case and paired multi-name selector orders. The reordered test kept PartnerKey at index 1, as in the original /choice; select a branch that actually moves index. |
| AJ05 | Unsecured baseline consumer, inherited bearer, security:[], [{}], optional anonymous branch; oracle checked anonymous with supplied credentials. | Covered by the combined existing consumers. |
| AJ06 | Missing credentials/grants challenged without effects; generic challenge validation; subsequent successful calls. | Add exact auth-family/name/grant-pointer assertions and one retained failed invocation → fresh corrected invocation → old invocation still terminal sequence. The helper dropped the old invocation. |
| AJ07 | Canonical origin spelling, wrong scheme/port/retrieval origin and malformed grants. | No attempt selected server B while retaining A-only grant. Add A success → B refusal → deliberate B grant success, checking destination/header values. |
| AJ08 | Missing server produced server-point challenge; selecting A succeeded; wrong-kind point challenged. | Missing explicit zero-effect assertion on destination challenge and selected-server-but-missing-auth stage. Combine with AJ07's short sequence. |
| AJ09 | Same-provider A/B calls then missing context; pending sibling cancellation/drop. | No same-provider cross-source/kind alternation. Also inspect actual A/B wire values; sequential calls only asserted success. |
| AJ10 | Supplied referenced scheme/mounted Path Item, conflicting secondary name, exact manual/cold generated wire, missing-resource refusal. | Missing authenticated cross-edition and ambiguous-resource negatives. Add mismatched secondary edition and conflicting identity or ambiguous mounted declaration; assert unavailable/refused and zero dispatch. |
| AJ11 | Unsupported mechanisms, mixed supported/unsupported, bearer roles, surviving unrelated operations. | Nonempty API-key roles were not exercised; add the corresponding mutation for both editions. |
| AJ12 | Named credentials; all compatibility carriers; bearer conflict/equality; malformed values, duplicate keys and grants; API-key control characters. | Add conflicting named/API-key-map/flat-key values. The conflict test covered only bearer. |
| AJ13 | Case-insensitive collisions, exact seed wire, error/debug sentinel checks. | Saved documents and coverage lacked explicit sentinel-absence checks. Maintained logs need the same final receipt check. |
| AJ14 | Real loopback 200/401/403/302; shared programmable status/malformed JSON tests. | Authenticated incomplete/truncated delivery state was not explicit; add one response with incomplete body evidence, one entry, no output, ERR_EXECUTION_FAILED. |
| AJ15 | Zero/extra input, cancellation before entry, pending cancellation/drop, sibling isolation, exact codes/counts. | Covered. |
| AJ16 | Context, retained metadata, header count, SDK error-size boundaries; repaired writer unit boundaries. | Main bounds were covered. Strengthen the context one-over assertion from merely is_ok() == false to exact ERR_REFUSED, matching the negative-case policy. |
| AJ17 | Source-only authoring, renaming, cold reload, resources and oracle invocation. | No positive assertions for OR/AND coverage, runtime prerequisites independent of schema loss, or preservation of unknown source fields. Add assertions around the existing synthesis result and one public unknown source field. |
| AJ18 | Extracted consumer, raw JS context/input seam, exact body/output, local Rc, existing isolation qualification. | No additional narrow auth assertion gap identified. Execution receipts remain coordinator evidence. |
| AJ19 | 1,000 shared mixed cycles/kind; owner baselines; host guards; cancellation/drop; cache reviewed as source-only. | Covered within the stated logical-owner scope. |
| AJ20 | Registered workload and qualification machinery. | Pending at the initial audit: coordinator reported preserved failed run and new CPU run pending. No passing conclusion from this audit. |
| AJ21 | Existing adapter/generic/consumer regression qualification; unchanged generic auth boundary. | Relies on current complete regression receipts; no new focused gap identified. |
| AJ22 | Decision ledger distinguishes experimental choices; bounded review repairs verified. | Coordinator must reconcile all required reviews and the acceptance gaps above. I did not read other reviewers' reports. |

The highest-value repair was a small shared test group covering AJ03/04, AJ06–10, and AJ17. Those assertions exercise materially different authorization or source-resolution situations; the existing happy-path receipts could not substitute for them. I explicitly did not make bearer-only POST a new blocking requirement: the literal AJ01 wording permits the existing operation-level bearer-plus-key POST.

## Verification of the grouped acceptance additions

I inspected examples/openapi-json-path-consumer/src/auth_acceptance.rs and verified that both native authenticated_consumers_and_lifetimes and Wasm authenticated_consumers invoke its suite for both editions. These conclusions concern the assertions present in the source. They are not claims that the new qualification run has completed.

The additions address the following initial gaps:

- AJ03/04: both combinations are invoked with both scheme orders; every selected credential and every selected grant is individually omitted, with CONTEXT_REQUIRED, zero host entries and no wire request. Both expected headers are checked with exact value/multiplicity. ServiceToken is selected across alternatives where its index changes from 0 to 2. Supplying both credentials without a selector still challenges without entry.
- AJ06: the missing-context challenge is checked for the source-declared bearer name and grant configuration pointer. The failed invocation is retained across a fresh authorized attempt, and remains exhausted with its original CONTEXT_REQUIRED outcome after exactly one new dispatch.
- AJ07/08: a missing destination challenges before auth and with zero entry. Destination A succeeds; changing only destination to B refuses A's grant. Selecting B without auth challenges without entry. Deliberately supplying B's grant succeeds at the exact B target with the expected Authorization value.
- AJ09: one provider alternates edition/kind and source bytes, checks the actual A/B/C token header each time, then requires new context rather than reusing the prior attempt's credential. Existing pending-sibling cancellation/drop checks remain in the shared cycle.
- AJ10: fresh authenticated reference mutations cover a secondary document of the other edition and a mounted Path Item with conflicting local operation content. Each asserts ERR_REFUSED and zero host entry. Existing positive reference and missing-resource assertions remain in the suite.
- AJ12: conflicts between the named key and each legacy/flat key carrier assert ERR_REFUSED and zero host entry.
- AJ13/17: source content and a public unknown source field are compared after save/reparse. Coverage asserts one AND branch with two requirements and OR branch sizes 1, 1, 0, plus the security prerequisite and existing schema-loss entries. Saved document and serialized coverage are scanned for the manufactured credential prefixes.
- AJ14: a host supplies valid JSON with BodyState::Truncated. The call asserts ERR_EXECUTION_FAILED and exactly one host entry; the shared drain helper checks that errors have no preceding output.
- AJ16: a long API key fills the exact default 65,536-byte encoded context allowance; that call succeeds, while one additional byte asserts ERR_REFUSED and zero entry. This supplies a stronger explicit credential-size boundary in addition to the existing bounds tests.

Two narrow assertions were absent at the first inspection of the grouped additions and were sent to the coordinator:

1. The new API-key-role mutation asserts invocation refusal, but AJ11 also requires honest unsupported synthesis. Add an ImplementationUnsupported coverage assertion for that target (and no generated callable target), as already done for bearer roles in the existing suite.
2. The new coverage checks use operations that already have source schemas and therefore legitimately produce schema-loss entries. They do not distinguish that behavior from an incorrect policy that marks all authenticated operations lossy. Remove the /items successful-response JSON schema in one fresh mutation, then assert that the authenticated target is Represented with its security prerequisite and has no Lossy projection entry. This tests the frozen plan's statement that runtime prerequisites do not themselves imply schema loss.

I then independently inspected the coordinator's final additions in the same file. Both points are now addressed:

- At auth_acceptance.rs:360, source-only synthesis of the API-key-role mutation must contain an ImplementationUnsupported entry for /paths/~1items~1new/post with no operation_key. The invocation refusal and zero-entry checks remain alongside it.
- At auth_acceptance.rs:459, a fresh source mutation removes the /items successful-response schema. The test requires a Represented target with the security configuration prerequisite and rejects any Lossy projection entry for that target. This directly separates runtime authentication prerequisites from schema projection loss.
- The test-only Clippy adjustment uses std::slice::from_ref and does not weaken those comparisons.

All material missing source-level acceptance assertions identified by this bounded audit are now addressed. The optional bearer-only POST case was correctly left out rather than promoted into a new requirement. This is static assertion closure, not a declaration that every acceptance gate has passed.

The maintained-log sentinel scan remains a final qualification-artifact check, rather than a runtime test assertion. I have not treated the presence of source assertions as proof of a successful new package or Node run.

## Execution and closeout limits

Earlier in this review I independently ran the two repaired challenge-writer unit regressions; both passed. I also inspected earlier coordinator package/native/Node receipts and checked their five relevant source/test hashes against the then-current files. Those earlier receipts do not automatically qualify the new auth_acceptance.rs additions.

At final static verification, the coordinator reports an earlier package-native run passed the grouped tests and 1,000 cycles, followed by a test-only Clippy finding that has been corrected. A new complete qualification run is active. I did not independently rerun those new consumers, inspect its unfinished receipt, or infer completion from the earlier run.

AJ20 remains known failed according to the coordinator's preserved CPU run 2: 38 of 40 rows pass, with both 64 KiB conjunctive-auth operation incremental-cost rows failing. I did not rerun or independently grade that CPU run. AJ22 remains pending the coordinator's final execution/review reconciliation, including the final package/Node receipts and log sentinel check.

This audit supplies no completion claim and does not relax the performance gate. Its final conclusion is: the consumer acceptance assertions identified in this review are present in maintained native/Wasm test paths, while the task remains an explicit performance-blocked checkpoint with final execution reconciliation pending.
