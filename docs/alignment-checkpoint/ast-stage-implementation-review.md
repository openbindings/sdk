# Independent implementation review: guarded AST stage

**PASS for the private, inactive AST stage.** This does not activate full VM preparation or close HIR/NFA, aggregate/default-workload, or construction-stack qualification.

Reviewed source: compiler/ast.rs, ast/tests.rs, compiler.rs and the previously qualified normalized guard. The implementation invokes the guard on the exact borrowed form, computes checked reservation terms from that returned inventory, charges the shared work/allocation controls, then constructs a fresh parser. Parser state is dropped before the completion check. On late cancellation or sticky refusal, Rust drops the held result, including an otherwise completed AST, under the prepaid cleanup reservation. Successful Parsed owns only the AST and scalar inventory. There is no parser, source string, control or partial shared program cache retained.

The reservation matches the independently reviewed site schedule, including the corrected 32*(N+r+k) visitor scalar term. Amount propagates overflow without wrapping. Work overflow preserves a prior cause and poisons an active enclosing Attempt through an impossible cumulative charge; allocation overflow uses the existing sticky hook. The initial schedule charge and prior checks occur before dependency entry. The 1,024-unit schedule allowance is independent of AST work and covers the fixed checked-arithmetic/layout operation count, not source traversal or allocation.

Backend representation/nesting errors are dropped without formatting or storing their owned source text. Grammar/depth, representation, target layout and shared-attempt failures remain distinct. The new stage has no production call path: compiler remains under the test-only inclusion while the complete certificate is open.

Evidence:

- Independently reran all eight stage unit groups: PASS. Exact/one-below work and allocation, no-parser-entry on failed precharge, prior/overflow sticky causes, late cancellation, clean retry, target constants, and all 1,142 unchanged Linear forms are covered.
- Added an independent counting-allocator probe importing the actual ast.rs and normalized.rs. It runs seven forms through exact limits, work/allocation one-below, every observed cancellation boundary, and a funded retry. The 151 observations on Rust 1.99 and the same 151 on Rust 1.88 are identical. All requested bytes stay within actual shared allocation charges, and all ownership is released after each call. The forms include overflowing two-bound counts, a 10,000-digit zero count, temporary empty groups/branches, ranges, empty input and the fixed boundary group.
- The parent’s 4,335-row current/minimum/actual-Wasm records additionally cover the unchanged full guard corpus and stage boundaries. Those records are supporting evidence; independent probe commands/results/hashes are retained separately here.

The initial independent driver used Cell in a Control implementor and failed its required Sync bound. That harness-only compile error is preserved as independent-driver-cell-error.log; the corrected driver uses atomic counters. It is not an SDK failure or oracle change.

Remaining qualification constraints: Layout::current checks public layouts; private GroupState/ClassState/visitor widths are source- and target-pinned constants. Word size alone is not a promise about every future Rust layout or arbitrary target. Before runtime activation, retain the explicit qualified target/build matrix and private-layout reproduction gate. The upcoming iterative metadata/HIR/NFA stages must not clone Parsed.tree or perform uncharged formatting; they should borrow the tree and drop it once under this allowance. Arbitrary caller Control/allocator behavior remains outside the SDK-owned allocation claim.

No production source was edited by this review.
