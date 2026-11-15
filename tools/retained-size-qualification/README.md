# Snapshot retained-size qualification

Native integration test of `Document::retained_bytes()` through the public API,
using a counting global allocator. This standalone test package permits the
allocator's required unsafe forwarding without weakening any SDK crate's unsafe
code prohibition. It is not a published SDK package.

Run `cargo test --locked --offline -j 2 --manifest-path tools/retained-size-qualification/Cargo.toml`.
The estimate must be within 25% of measured live allocation bytes, both immediately
after parsing and after validation and lazy document/schema/location lookups.
The workloads are a small typical document, a node-dense extension array and the
unchanged `acme-refunds.obi.json` copied from Panjir's system-worker fixtures.
Process-wide packaged schemas are warmed before accounting, since snapshots do
not own them. Requested allocation sizes exclude allocator overhead, as does the
API. Clones and proofs charge the same snapshot. Dropping all owners releases
its measured allocations. One-off measurements belong in the decision receipt.

`context_bytes` also qualifies `ComparisonContext::retained_bytes()` at 10, 100
and 500 operations, before and after comparison builds its reference indexes.
It measures the context alone after the proof drops, including supplied resources
and aliases sharing one source arena. The context estimate must stay within a
factor of two of requested live bytes. This empirical qualification is not a
universal bound. Clone stability, schema-text coverage, growth and release of
every measured allocation are checked. The original document tolerance remains
25%. Run with `-- --nocapture` to print each estimate beside its allocator count.
