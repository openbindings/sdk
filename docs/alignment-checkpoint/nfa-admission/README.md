# Inactive actual-HIR structural admission

The independent [fold design](design.md) is implemented as an ancestor-only walk
with three output scores: expanded compiler dispatch, helper input payload and
empty-tail join pressure. Arithmetic caps finite scores with zero annihilation;
only the completed root determines per-program admission. Unicode range
enumeration uses one lazy reused scratch owner. Unknown look modes refuse rather
than inheriting the closed profile.

Five focused groups pass all 37 frozen hand-counted cases, arithmetic extremes,
Unicode scratch reuse, cancellation boundaries and shared failure. The larger
11,274-row isolated-HIR replay matches all three independent expected scores on
current/minimum Rust and actual Node, Chromium and WebKit. All measured inventory
scratch is released; its observed requests fit the SDK reservation. This is a
scoped scratch-owner result, not a compiler-wide allocation claim.

Current evaluator library tests pass 160 groups with two existing release-only
tests ignored. Minimum Rust passes all 44 compiler groups; strict Clippy passes.
The [single-build owner](../single-build-nfa-owner/review.md) separately passes
independent review, including the corrected failed-allocation cancellation path.

Independent implementation review now passes at `34f898c`: 11,274 independently
replayed rows plus 448 extra controls on each Rust version. See [the review](independent-review.md).
The later [joined factory](../joined-pattern-factory/README.md) wires these scores
into the single-build owner before dependency entry. Shared construction aggregates, retained unique
program storage, full transient/time/stack boundaries and default-engine activation
remain open. These modules are test-only; current ordinary evaluator behavior is
unchanged by this checkpoint. Inputs include synthetic and upstream-refused HIR
forms to test the isolated fold and do not claim those source patterns are admitted.
