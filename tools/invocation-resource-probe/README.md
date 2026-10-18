# Invocation resource probe

This standalone, unpublished public API consumer measures the same pure exact
echo/empty fixture directly and through the checked invocation lifecycle. It
uses one thread, a small safe executor, the shared Cargo target, and no network.
The SDK retains its unsafe-code prohibition. Only this isolated measurement
executable implements `GlobalAlloc`, forwarding every allocation unchanged to
`System` with audited, allocation-free atomic accounting.

Run from the SDK root with the configured Rust toolchain and cached Cargo home:

```sh
python3 tools/verify-invocation-resources.py
```

The verifier builds release mode and writes the current receipt and command logs
under `target/invocation-resources` (or the existing `CARGO_TARGET_DIR`). The
checked-in Cargo lock fixes the standalone dependency resolution. Structural
gates always apply. `--record-baseline` records a first host-specific baseline;
it refuses to replace an existing one. `--check-timing` additionally checks its
provisional budgets, defined as the larger of three times initial time or the
initial time plus 2 microseconds per call. Timing comparisons qualify only this
synthetic host and toolchain; no performance parity is asserted.

Measurements cover handle construction/release, zero outputs, unary and 64-value
streams at 1 byte and 64 KiB, a 1-byte selected subtree backed by a 1-MiB owner,
and cancellation/release. Constructing and releasing an operation invoker must
allocate nothing. A 1-MiB source is borrowed unchanged for all checked
calls. Per-call allocation volumes and maximum individual allocations provide
evidence against serializing/reparsing that source on each invocation. Native
queue and pending-item maxima are qualified separately by SDK unit tests.

The fixed retention gate performs 10,000 completed and 10,000 cancelled calls
after warmup, drops retained outputs and backing values, and requires zero live
allocation/byte growth and zero live provider guards. Each branch verifies one
provider start per call independently. Exact output counts, order, values and
terminal success are checked during every echo call; cancellation must terminate
once with `ERR_CANCELLED`.

Counters report successful `System` allocation/reallocation requests. A realloc
counts the full new reservation as allocated and the old reservation as freed;
it is not a measured byte-copy count. Timings include counter atomics, guard
atomics, the checked provider's boxed future and the safe executor. Setup and
receipt encoding are excluded. Logical bytes and retained parent bytes are
reported separately: queue byte caps are not total-memory bounds, and allocator
counts are not browser/process RSS measurements.
