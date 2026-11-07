# Compact execution over original schemas

This increment removes unneeded per-value state; it does not change public APIs,
raise resource defaults, select the original engine as default, or close iterative
traversal/diagnostic/compiler gates.

Preparation computes conservative property/item annotation demand for the entire
immutable activated graph, including all known dynamic/recursive candidate targets.
A known unevaluatedProperties consumer demands property annotations everywhere;
unevaluatedItems similarly demands item annotations. Unknown contents remain opaque
validity effects. No later inventory expansion can consume an omitted annotation
without preparing a new Plan. Each Plan::at computes its own demand.

When a coordinate kind is unobserved, execution does not allocate its maps, create
recursive coordinate cuts, discover its support, or certify an omitted coordinate
as false. A missing unobserved map is not evidence of annotation absence. When a
kind is demanded, retain the current full conditional/failure-gated annotation and
recursive refinement behavior. Local additionalProperties selection and contains
match counting remain necessary even with outward annotation maps disabled.

Preparation also certifies leaves with eligible inventory and a known profile.
A leaf has no active reference or evaluation-applicator keyword at all. Presence,
not discovered child count, disqualifies it: empty anyOf/oneOf and malformed active
operands still have behavior, including under permissive custom meta-schemas.
Use the shared child-keyword inventory plus foreign dependencies/additionalItems;
conservative exclusions such as orphan branches and content schemas are acceptable.

A certified leaf still checks work/depth, enters the exact dynamic scope, and runs
primitives, format and pattern interpretation. It creates no Visit/active/memo
record and skips irrelevant container/applicator traversal. Local undefined and
inability effects retain original identities. Non-leaves keep current completed
state memoization and active occurrence/scope identity, preserving reference
sharing and recursive certificates. No allocation multiplier is simply reduced.

Qualification must restore the unchanged 20,000-element valid/late-invalid arrays
and 50,000-member integer object at default limits through forced original execution.
Add direct large controls, absent-versus-present annotation consumers, original
references/dynamic/custom profiles, empty/malformed applicators under permissive
custom meta, cancellation/limit/retry and reference-diamond controls. Re-run current
semantic/recursive/coordinate/required corpora and supported native/host lanes.
Independent review must explicitly check omitted-map certification and leaf
classification before this increment is accepted.
