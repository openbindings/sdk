# Prepared active-keyword presence

Status: bounded design for the next performance increment, after the completed
432cf15 campaign. That campaign has 101 correct observations but 32 latency
regressions under the frozen noise policy. The constant-wrapper experiment is
rejected by isolated measurements; Plan-backed source borrowing is retained as
an ownership cleanup with no material speed claim.

The evaluator keeps one original-node Plan and the same application/predicate
algorithms. Add `semantic::readings` for a compact, immutable active-keyword mask.
This is private prepared interpretation metadata, not another schema tree,
semantic identity, public SDK concept or evaluator.

## Representation and ownership

Replace `Plan.leaves: HashSet<usize>` with one `HashMap<usize, readings::Fields>`.
A Fields word holds the closed recognized keyword bits, the existing certified-leaf
flag, and an explicit Original flag. A missing record is Original with no leaf
certificate. Known mask zero means all recognized keywords are absent. A present
bit means read the original operand, including any malformed shape. The optional
numeric cache remains independent: its misses/caps still mean Original readings.
No empty full operand records, copied JSON, per-key Arc, slot vector or dense table
for unused resources is added in this increment.

Cache the Fields word in each existing application Frame. Interpreted::get uses
that word; it must not hash the node for each keyword. Present keywords and all
Original cases still call Profile::get. Names outside the closed bit inventory
also fall back to Profile::get. Refinement uses exactly the existing certified-leaf
flag, never absence of other metadata as a substitute.

## Construction

The current preparation loop already visits every original member after admitting
an activated, eligible, known-profile node. Accumulate a separate provisional word
there. For each recognized decoded member name, use Profile::get to establish
activation before shape/child filtering. Profile::get is a filter over the actual
schema member and synthesizes no defaults; the complete scan therefore establishes
absence without 43 unconditional lookups per node. Unknown/non-scalar names cannot
become keywords. Escaped equivalent names behave as the existing getter does.

All preparation reads remain Original until the entire member scan finishes. Do
not filter later siblings, if/items applicability checks or source traversal using
a partial mask. Publish only after a final shared-control check, retaining the
same leaf classification. Unknown/ineligible positions get no presence certificate.
Boolean-before-profile behavior and all original fallback paths remain intact.

Charge the new map entries/growth through the existing construction reservation
policy before allocating/publishing. Account non-leaf records as additional storage;
leaf records replace the old leaf-set entry. No new public limit or enlarged work/
reservation default is authorized. The shared Attempt is never reset, and failures
cannot be caught and converted into an absent-field certificate.

## Execution

Route through the existing References, Logical, Object and Array phases. Omit a
whole phase only when its active keywords are certified absent. Keep the exact
original operand, ordering, applicability, query, effect and annotation logic for
every present keyword. In particular empty anyOf/oneOf, malformed custom-admitted
applicators, orphan conditionals, legacy dependencies/additionalItems and contains
bounds cannot disappear because no child was discovered.

No value-based early branch skipping, control throttling, static alias flattening,
new numeric algorithm or new object/array child graph belongs to this increment.
Such changes need separate evidence. Removing nonexistent phase work may reduce
minimum work/checkpoint totals; real-loop progress, entry/completion checks,
interruption recovery and first-cause behavior remain required.

## Validation

Apply the independent static review's 20 control groups with the mask-only
refinement. Cover original versus known-absent/present, complete publication,
custom/edition activation, malformed/empty/orphan applicators, exact operands and
numeric-cache cutoffs, scope/reference/cycle/effect correlation, annotations,
source lifetime, all interruption boundaries, partial cleanup and declared limits.
Run the full evaluator kit without changed semantic expectations.

Retain the original default workloads: 20k and late-invalid arrays, 50k object,
700k integer array, reference1000/5000, tiny/count patterns, 380 long literals,
5000-shorthand single/four/repeated380, opaque resources and large const. No capacity
loss may be hidden by increasing a default. Recalibrate exact work/storage boundaries
only where actual implementation work changed and preserve the old observations.

Measure the same selected requests before broader sampling. Performance findings
remain open until the final candidate passes the frozen gates or has explicit,
bounded, independently reviewed tradeoffs. Semantic PASS alone does not close G6.
