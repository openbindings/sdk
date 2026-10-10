# Internal dependency fork

Based on `jsonschema-0.58.6`; upstream license and notices are retained.
This distinct package name prevents unpatched transitive dependency resolution.
Changes are documented in the task foundation patch manifest; production changes
and regression tests must be recorded in the root dependency maintenance ledger.
This package is an implementation detail, not a supported SDK extension API.

The repository maintenance contract is DEPENDENCY-MAINTENANCE.md. Original
upstream identity and manifest are retained as UPSTREAM-VCS.json and
UPSTREAM-Cargo.toml; these names avoid Cargo-reserved packaging metadata.

The SDK value-diagnostic metadata scope admits copied instance/member strings
before allocation, omits unused nested/payload diagnostics and records deterministic
usage counters. It is active only during the default evaluator's post-verdict
diagnostic pass. See the root maintenance contract and evaluator resource guide.


Metadata-mode property-name validity/validate/evaluate rechecks preflight copied
names before `with_string_node`, including nested applicator paths. Scratch refusal
poisons diagnostic construction, while normal verdict evaluation is unchanged.
Opt-in required-name metadata captures the already established missing member only
after scratch admission; optional refusal is separate from base diagnostic refusal.
The SDK subsequently verifies membership in the original required array before
exposing it. No other source operands are copied into vendor error payloads.
