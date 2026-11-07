# Evaluation scratch reuse checkpoint

Per-evaluation reference traversal now reuses its empty path backing. Scalar and
already-known value interning starts inline and allocates a pending vector only
when a container requires it. No public API or semantic identity changed.

The full evaluator suite and strict clippy pass. Independent review passes all 17
frozen groups on Rust 1.99 and 1.88, including interrupted cleanup, collisions,
dynamic/recursive scope and partial identity publication. See the retained review
and control sources. These are bounded checkpoint checks; refreshed full host
qualification remains required for the final candidate.

The unchanged allocation pilot reduces reference-text-5000 from 10,007 allocations
to 7 and unique-array from 20,045 to 45. Complete observed outcomes are preserved.
Large composite literal and uniqueness storage remain open performance findings;
this checkpoint grants no G6 pass or timing tradeoff.
