# Checked pattern inventory checkpoint

The private iterative HIR inventory is implemented and remains test-only. It has
bounded independent design and [implementation/resource approval](pattern-inventory/global-collector/review.md).
This checkpoint does not activate a compiler or change the evaluator default.

One ancestor frame vector folds the guarded AST. Frames contain only root-shape
facts and descendant alternation/range/literal-byte counts used by parent formulas.
A walk-owned collector adds other counts and construction sites once at their local
node. The result owns the same parsed AST with its complete checked work/allocation
schedules and prospective prefix/equality heights. A cancelled or exhausted attempt
publishes no certificate. Wide shallow inputs need constant frame depth.

The first representation unnecessarily merged additive work/site totals at every
ancestor. The independently approved collector removes those repeated operations
and shrinks the native frame from 344 to 184 bytes. No old public API or internal
layout required either representation.

Parent validation covers 18 independent controls, all 1,142 retained Linear forms
and 10,065 generated inventory summaries, every work/allocation site and prospective
height. Current/minimum Rust have identical results and requested allocations, with
no observed undercharge or retained bytes. The original generated raw-scalar forms
include 8,253 guard refusals; a separate lossless uppercase scalar-escape spelling
lane preserves the original inventory expectations. No oracle was derived from this
implementation or changed to manufacture a pass.

Actual Node, Chromium and WebKit runs use a 128 KiB Wasm linear stack: 11,269 input
rows include 44 Reader noncomparisons, 11,206 successful summaries and 19 independently
predicted checked 32-bit work-sum refusals. Those generated adversarial refusals are
not successful summaries or required-workload passes. No retained allocation or
whole-stage undercharge is observed. Failed inventory consumes its AST; that AST's
cleanup belongs to its earlier prepaid phase, not frame-only storage.

The SDK includes the 18 independently authored expected records and tests every
field/site. Current library tests pass 138 groups with two existing release-only
stack tests ignored; the nine inventory groups pass on minimum Rust. Strict
all-target evaluator Clippy passes. [Source identities and bounded receipts](pattern-inventory/qualification.json) are
saved alongside this checkpoint; whole compiler/aggregate/stack qualification is
still required before activation.

The direct iterative AST-to-NFA alternative was separately prototyped and rejected:
its naive Unicode/shared-prefix representation produced substantial search and
cache costs. The retained dependency route is selected on evidence and remaining
implementation burden, not compatibility. The existing projected evaluator and
its obsolete APIs are still scheduled for removal after replacement qualification.

Next: checked HIR translation and NFA stages, qualified construction-stack admission and complete default workloads.
After those gates, force the original evaluator through the final semantic/host/
performance checks, switch the default, remove the outgoing path, and finish the
spec-to-code audit. No release or merge is authorized by this checkpoint.
