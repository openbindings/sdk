# Current source-pull catalog scale checkpoint

The current external consumer completes 0, 100, 1,000 and 10,000-item catalogs
through source projection, checked pulling, prospective conformance and caller
serialization. Seven repetitions of six phases produce 24 observed rows. Every
call must complete with the expected report, operation/binding counts and unchanged
original snapshot; refusals are failures, not throughput samples.

The original default run refused the 10,000-item case. Its output and diagnostic
threshold probe are retained. Independent review derives the B-tree height from
the supported Rust implementations, reducing the work reservation without reducing
allocation reservations. The corrected catalog still needs 46,765,436 work units,
so the explicit SDK policy now provides 64,000,000. The default 64 MiB allocation,
100,000 entries and 32 MiB handoff caps are unchanged. See the independent
[catalog-budget review](source-pull-catalog-review.md)
for exact/one-below controls and source proof. No spec requirement sets these SDK
defaults.

The source-pull crate passes 27 tests/doctests and strict lint. The external
archive consumer now includes a permanent 10,000-item default-policy test and
an explicit insufficient-budget refusal. Rust 1.99 and MSRV 1.88 native/archive/Wasm/Node qualification both pass;
current Rust also passes strict lint. Each lane records its own package/input
identities and isolated build directory.

Descriptive 10,000-item medians in this run: projection 47.198 ms, complete checked
pull 71.886 ms, relationships plus prospective conformance 24.720 ms, cold
conformance 6.708 ms and serialization 2.197 ms. Cached conformance is below useful
display resolution here. These observations were not collected in a reserved quiet
window and do not close the final matched performance campaign. Provider and core
conformance allocation are outside the documented relationship ledger.

Recovery: run `tools/verify-source-pull.py --archives --wasm`, then build/run the
generated consumer's `measure` example in release mode using that verifier's
isolated build directory. The versioned fixture source carries the workflow;
generated receipts preserve the package and input identities.
