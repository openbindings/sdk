# Original evaluator: concrete contains counting

The first original-engine increment counts concrete matches without allocating
a formula distribution. Actual uncertain matches retain the existing coherent
formula reasoning, with the concrete count as an offset. Repeated and complementary
effects remain correlated; per-item evaluated annotations are unchanged.

The native evaluator suite passes 259 tests/doctests (three existing release-only
stack controls ignored in debug), strict lint and the required-value replay. New
controls directly exercise the original-node Plan at 128/1,024/4,096 items, exact
min/max boundaries, late invalid items with unevaluatedItems, zero-allocation
concrete counting and shared uncertainty. Rust 1.88 passes the three new controls.
The rebuilt Wasm package passes all 180 Node tests, installed-package consumers,
and both Chromium/WebKit independent judgments (505 core and 1,546 evaluator values
per browser). These inventories overlap.

This does not switch all schemas to the existing semantic Plan or close the ordinary
compiler allocation/performance gate. Prepared original instructions, compact
annotation demand, iterative execution, original diagnostics and the final matched
performance campaign remain in the reviewed sequence. Independent review passes at da39d8b: both Rust toolchains match 2,465,940
truth-table/range comparisons, 200,000 concrete additions allocate no heap, and
46 cancellation boundaries plus four resource-refusal controls preserve clean
retries. No whole-SDK completion is claimed.
