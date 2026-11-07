# Original schema execution with explicit frames

The independently reviewed design is implemented across every schema application:
references, logical branches, conditionals, object members/names/dependencies,
array selection/counting and unevaluated coordinates. A single bounded vector
holds unfinished applications and a return slot transfers each completed child.
No recursive schema fallback remains. Exact predicates, coherent effects and the
reviewed recursive certificates continue to determine meaning.

Effective dynamic scopes are immutable shared sorted maps, compared by content.
Unchanged scope entry does not copy binding names. An explicitly sized ordered
hash path supplies active suffixes and alias membership; its two vectors reserve
full replacement backing before growth. Completed states and provisional repeats
remain distinct. A dedicated abort signal abandons unfinished parents and commits
facts only at the existing between-attempt boundary. Actual limits/cancellation
are terminal. Alias targets with certified leaf behavior finish without retaining
per-item frames or memo entries.

Independent review found a missing owned-string charge for the recursive-scope
name and recommended a non-inlined inner call beneath the stack anchor. Both
are corrected. The new sampled stack guard is call-local and sticky across
attempts. Existing bounded symbolic depth remains. Observations demonstrate
refusal and small-thread behavior, not a hard process-stack cap; native/Wasm
layout and full physical-stack qualification remain open.

## Qualification recorded so far

The 36 controls fixed before implementation now pass, including the four formerly
failing depth32/64 outcomes. Added controls cover collision chains/rehash/pop/
clear, failed growth, exact frame backing limits, content-keyed scopes, cancellation
at observed transitions, 50,001 alias-reached values and undefined-pattern
correlation under distinct scopes. The original 20,000/20,001/50,000 container
controls remain present and pass.

Final current Rust tests pass 533 checks across core, internal schema and evaluator;
six ignored checks are separately identified (four existing release controls and
two new release-only observations). Strict Clippy passes. Minimum Rust 1.88 passes
the first-source 530-check campaign and the final-source 30 semantic checks; both
compilers pass the two new release stack tests, including depth256 on a 128 KiB
thread. Full earlier/source identities are distinct in the receipts; inventories
overlap and are not additive semantic coverage.

Fresh Wasm passes 181 Node tests, type checks, 352 installed-package assertions
and both actual browser lanes. Each browser judges 505 core cases and 1,546 required
values. Eight new shared host controls exercise depth32/64/256 valid/invalid
values and 50,001-item alias jobs through an actually supported custom profile,
which selects original-node evaluation without a testing-only switch. The saved
`6988e6d` npm package fails the first depth test with its former semantic-depth
refusal; that negative control is preserved. Host identities and final source
hashes accompany the results.

Independent bounded implementation review passes at its final source hashes. Its
replay passes all 1,546 required values, 36 controls, 62 numeric/container controls,
17 chain/alias controls and four 2019 recursive-reference controls. It also compares
770 generated applicator/annotation cases to the prior reviewed Plan. The initial
and corrected snapshots are preserved by the reviewer; final-source claims belong
to its final report. It checks 4,830 cancellation boundaries, an 8,000-operation
forced-collision model and eight concurrent owners each running 16 cancellation/
reuse pairs. Both new release stack checks now run in the portable verification
command on every native CI host.

The frozen repeated-reference-text preparation jobs remain refusals, with unchanged
inputs and allowances. Full pattern VM preparation, syntax-fact sharing, original
Explain witnesses, the default-engine switch/compiler retirement, matched final
performance and the final alignment audit remain open. This is a bounded
implementation checkpoint. No release, deployment or merge is performed.
