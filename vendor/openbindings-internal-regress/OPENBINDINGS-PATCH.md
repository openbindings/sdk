# Internal dependency fork

Based on `regress-0.12.0`; upstream license and notices are retained.
This distinct package name prevents unpatched transitive dependency resolution.
Changes are documented in the task foundation patch manifest; production changes
and regression tests must be recorded in the root dependency maintenance ledger.
This package is an implementation detail, not a supported SDK extension API.

The repository maintenance contract is DEPENDENCY-MAINTENANCE.md. Original
upstream identity and manifest are retained as UPSTREAM-VCS.json and
UPSTREAM-Cargo.toml; these names avoid Cargo-reserved packaging metadata.
