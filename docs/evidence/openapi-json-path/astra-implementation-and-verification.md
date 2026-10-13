# Independent Astra implementation reviews

Root-authored summary of reviewer messages; not a verbatim transcript.
Both reviewers ran GPT-6 Astra at xhigh with separate clean context and no old drafts.

## Contract / effects reviewer

Initial material finding: exact integer helper incorrectly refused 10.0e-1,
100.0e-2 and 100.00e-2. Independently compiled the actual helper to reproduce it.
Also requested manual mounted-path and Unicode/tilde acceptance evidence.

Verified repair through both public binding and generic operation/evaluator regression.
Manually authored native mount passed. Literal Unicode initially failed and was
independently reproduced; encoded Unicode later passed. Reviewer explicitly says
the encoded route is not a pass for literal Unicode.

Final focused review: no remaining material finding in accepted Claude repairs.
Independently ran response-profile/ambiguous-mount coverage, required-value/
duplicate/media/body-budget tests, and native manual mount/encoded Unicode.
Role/core boundaries remain unchanged. Literal Unicode and then-pending performance/
final package evidence were excluded from that approval.

## Consumer / resource reviewer

Initial findings: full effective descriptor retention escaped aggregate byte accounting;
parameter limits applied after costly conversion; finite call helper masked closed-write
terminal errors and checked cardinality before item errors. Metadata multiplier was a
static calculation, not an executed RSS measurement.

Focused verification found all three code repairs appropriate. Ran metadata-budget and
integer regressions; initially observed the literal Unicode failure before the helper's
missing-operation assertion. Later native run reached and passed that assertion.

Final harness review found: stage decomposition absent; paired overhead not actually
paired; receipts could hash newer checkout files than those built; timing accepted a
failed qualifier's extracted directory. Owner repaired all four. Reviewer inspected the
repairs without disturbing timing. Final nuance: every matched pair must meet the
ceiling although the display also contains an aggregate-median ceiling. This stricter
rule is explicitly documented in the final evidence report.

Remaining qualification limits: stage timings are independent diagnostics, not an
additive decomposition; logical-owner baselines are not RSS; copy counts and peak
retained-value bounds are structural evidence, not measured allocator peaks.
