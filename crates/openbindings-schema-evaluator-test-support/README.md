# OpenBindings evaluator test support

Run this finite suite against a custom `SchemaEvaluator` through the public document/context route:

```rust
use std::sync::Arc;
use openbindings_schema_evaluator_test_support::{run, Options};
let evaluator = Arc::new(openbindings_json_schema_evaluator::DefaultEvaluator::new());
let report = run(evaluator, &Options::without_unicode_property_matching());
assert!(report.is_success());
```

The package carries 496 groups and 1,566 cases from pinned upstream JSON Schema and Go SDK adversarial inputs. `fixtures/provenance.json` records their identities, exact optional capability cases, and historical translations, including exact identifier matching and non-exhaustive diagnostic multiplicity. Library URI normalization supersedes the historical spelling policy through recorded runtime migrations. The original licenses accompany the fixtures. No sibling checkout or network access is needed.

The raw fixture remains unchanged. `fixtures/document-migrations.json` re-expresses
nine custom-meta-schema cases through conformant documents referencing supplied
resources (§5.2), preserving `$id` and every refusal reason/code. Diagnostic locations
now identify the supplied resource. No raw evaluator seam is needed. The two
`adversarial/unreached-uncopied-targets/{0,1}` cases move to core OBI-12 rejection
tests because no public normative API can reach their former satisfies/fails states.
The [full migration ledger](../../tools/qualification/inputs/s6c-migration-ledger.json) accounts for all 1,566 original IDs; the runtime kit has
495 groups, 1,564 observations, 1,522 verdicts and 42 refusals.

The raw fixture remains unchanged. Its provenance also records explicit
`runtime_policy_translations` entries for the current SDK admission policy:
`adversarial/resource-uri-names-another-id/0` now expects satisfaction from the unique
contained integer schema, preserving the historical conservative-refusal expectation
in the fixture. Native and browser qualification apply the same recorded translation.
This is an intentional supported-admission expansion, not a normative bug ruling.
Library URI normalization additionally migrates `adversarial/case-variant-unmatched/0` to satisfaction
and `adversarial/case-variant-collision/0` to ambiguous-resource preparation refusal,
following library normalization under applied §7.4 (Amendment 1). OBI-13's exact
comparison remains unchanged.
The current default evaluator records 42 refusals across 1,564 evaluator observations;
competing-carrier alias/pointer/direct routes have separate maintained regressions.

The report records preparation refusals separately from optional value outcomes. A preparation refusal does not manufacture a value-validation result. Permitted refusals are counted separately from validity verdicts. Options name exact cases with their predeclared capability reason. Unknown, blank, stale or unused declarations fail; a wrong Boolean verdict can never be exempted. Diagnostic checks verify every supplied instance path and original schema location. Empty incomplete problems and first-error-only diagnostics are allowed; detail is never verdict evidence. The kit itself has injected wrong-verdict, bogus-path and misclassified-cancellation controls.

`examples/boolean_evaluator.rs` is a small custom evaluator that consumes the original context and honestly declines non-boolean schemas. It demonstrates integration rather than claiming full capability. An integration test wraps the default evaluator with an original-context assertion and runs the complete suite.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Package publication and application cutover are separate actions.

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](../../docs/api-reference.md).

The kit checks preparation and value outcomes, original locations and refusal
causes. Prepared objects expose no completeness declaration. The qualification
kit does not impose the default evaluator's private planning policy on a BYO evaluator.

The diagnostics mode migration preserves every verdict/refusal and records each changed observation in
[`diagnostics-migration-ledger.json`](../../tools/qualification/inputs/diagnostics-migration-ledger.json).
Both Coarse and Upstream are replayed; incomplete diagnostic subsets never imply
missing rule evidence or authorize a wrong verdict/refusal.
