# Measurement-resolution design comparison

Design-only checkpoint, 2026-10-13. Product/runtime and every recorded performance
outcome remain unchanged. The authenticated slice remains performance blocked.

Start with the [synthesized execution proposal](synthesized-plan.md). It combines
two independently authored proposals and a focused reconciliation. Claude accepts the
reconciled plan as ready for implementation review. It is ready for
implementation review after execution is authorized; it does not authorize timing.

- [Coordinator's original proposal](coordinator-design.md), committed at 1d2f142 before
  reading Claude's new proposal.
- [Claude Opus 5.5 Extra's original proposal](claude-design.md), authored without reading
  the coordinator's proposal, from the same prior evidence.
- [Comparison and design choices](comparison.md), including weaknesses in both.
- [Claude's synthesis review](claude-synthesis-review.md), including final reconciliation.
- [Design arithmetic checks](design-verification.json): schedule/count checks only,
  no SDK build, test or benchmark execution.

The selected method compares exact same objects, distinct objects sharing immutable
input/response storage, and distinct objects with separately allocated equivalent
storage. It preserves complete timed call work, retains objects through label swaps,
and checks both construction orientations. Method selection uses controls only.

One fixed 96-configuration control study can permit one complete qualification. Every
original performance budget, per-pair requirement and historical failure is preserved.
The 1 µs control guard is unchanged. A new explicit conservative completion check also
requires an auth pass's margin to exceed the observed component-wise null envelope;
that envelope is not a statistical confidence bound.

The larger control set needs more observation per difficult row: 800 ms per condition
per sample for AND 64 KiB, 200 ms elsewhere, seven samples throughout. The planning cost
is approximately 40 minutes for controls and, only if they pass, approximately 40 minutes
for qualification; each phase has a 45 minute hard timing cap. No post-data tuning or
automatic third study is permitted. Those caps and the stricter completion check are
part of the proposed plan for future authorization, not changes applied to old data.
