# SDK performance boundaries

Duplicate detection hashes decoded UTF-16 units and compares only distinct names
in the same hash bucket. Equal repeats retain their first lookup slot, while all
original members and duplicate token coordinates remain in the arena. Objects
with at least 16 members retain these buckets after parsing for pointer and
evaluator lookups. Smaller objects scan at most 15 members. The retained index
stores member positions, without copying decoded names, and is immutable across
retained views and threads.

For ordinary hashes, parsing costs O(source bytes plus decoded key units), and
lookup costs O(token units plus matched key units). Distinct keys deliberately
colliding in one hash bucket require up to the bucket's distinct-name count in
exact comparisons per insertion or lookup. Each comparison is bounded by key
length; caller byte and node admission limits bound both lengths and bucket
population. RandomState uses a per-arena random key, but does not guarantee a
constant-size bucket. Array indexing and RFC 6901 decoding keep their existing
semantics. The evaluator charges the indexed table access and every candidate
comparison against its existing work budget.

The manual measurement is in
`crates/openbindings/tests/indexed_lookup_measurement.rs`. Run it with the
development Rust toolchain and no concurrent compilation:

```sh
cargo test --release --locked -p openbindings --test indexed_lookup_measurement -- --ignored --nocapture
```

Each fixture has two warmups and five measured assessments, with the median
reported. Input generation, result destruction and verdict assertions are outside
the timer; parsing and all document assessment work are inside. The duplicate-member
fixture has N distinct keys followed by the same N keys inside `x-e`. The
schema-reference fixture has N schema entries, each referencing the last schema.
These are measurements, with no timing acceptance assertion. Deterministic
regressions at N = 10,000 / 20,000 / 40,000 assert N repeated-key comparisons,
zero distinct-key comparisons, N successful indexed member probes, and 3N probes
for reading and resolving all schema references.

Geometric hash-table capacity explains the steps in retained memory. Across an
arena the index retains O(distinct object names plus indexed objects) slots, with
an additional O(colliding distinct names) vector payload when collisions occur.
The parser moves existing buckets into the arena without building a second copy.
