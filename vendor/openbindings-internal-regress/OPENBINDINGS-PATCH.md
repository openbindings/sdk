# Internal dependency fork

Based on `regress-0.12.0`; upstream license and notices are retained.
This distinct package name prevents unpatched transitive dependency resolution.
Changes are documented in the task foundation patch manifest; production changes
and regression tests must be recorded in the root dependency maintenance ledger.
This package is an implementation detail, not a supported SDK extension API.

The repository maintenance contract is DEPENDENCY-MAINTENANCE.md. Original
upstream identity and manifest are retained as UPSTREAM-VCS.json and
UPSTREAM-Cargo.toml; these names avoid Cargo-reserved packaging metadata.

Evaluator correctness: `src/classicalbacktrack.rs` preserves
zero repetitions when a single-character loop's atom cannot be represented by
the input element type (notably lone surrogates on scalar strings). Lazy loop
extension likewise consumes nothing. Positive minima still fail; UTF-16 input
can still represent surrogate code units. Both interpreter budget exits clear
the backtrack stack to its exhaustion sentinel, preserving debug invariants.

`tests/tests.rs` directly exercises budget interruption without `catch_unwind`
and checks every row of `tests/fixtures/lone-surrogate.tsv`. Regenerate the 470
rows with `node tests/fixtures/lone-surrogate.mjs`; expectations were computed
with Node v22.19.0 / V8 12.4.254.21-node.29, `new RegExp(pattern, "u")`.
Coverage includes zero/positive minima, lazy quantifiers, classes, alternation,
lookbehind, paired surrogates, and braced Unicode escapes. The SDK also checks
each fixture row through both schema pattern keywords. No upstream upgrade is
included; these are local correctness and budget-invariant repairs.
