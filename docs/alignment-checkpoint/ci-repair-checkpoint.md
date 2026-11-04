# Current interface packaging and CI repairs

Checkpoint after `b380d61`. This receipt records repairs to actual hosted CI
failures; independent review and full clean-candidate validation are pending.
No package publication, main merge or completed SDK alignment is claimed.

The fixture checker accepts both current two-column and three-column digest
ledgers. Four new companion ledgers cover every vendored file and identify
published inputs separately from SDK controls. Every published entry was checked
against the pinned interfaces. Independent review found that deleting an entire
ledger could bypass discovery. The checker now requires all six current interface
ledgers, retains discovery of additional ledgers, and tests deletion of each.
All 32 files pass; drift, absent rows, absent files and duplicate rows still fail.

The context, kind-description, schema-comparison and source-pull archives now
carry the repository license and applicable attribution notice. Stale notices
name the actual pinned core revision; historical copyrights remain. Package-list
checks show LICENSE and NOTICE in all four archives. Full clean archive consumer
validation remains pending.

The standalone evaluator benchmark lock adds only the current IDNA/ICU closure,
using the exact registry versions already present in the root lock. Locked offline
build/check passes. The preparation-refusal counter now exercises a real resource
refusal (schema_resources = 0); the original missing local reference remains a
separate Undefined value-outcome control. All nine observation tests and 48
comparison-policy tests pass. The comparison guard is unchanged: third-party lock
changes can make the old/new pair not comparable. Final matched performance still
requires the planned controlled instrument and semantically matched workloads.

The new numeric results record uses a neutral minimum_rust field with an explicit
version; the first label-check failure is preserved. The recovery plan corrects
stale stage-status prose without changing the implementation sequence or gates.

The full local verification first hit denied local test-server access. A permitted
rerun cleared that environment failure and remains in progress. Original failure
logs are retained. No assertion was skipped or relaxed. The independent review is
recorded separately under ci-repair-review and will close or correct this checkpoint.
