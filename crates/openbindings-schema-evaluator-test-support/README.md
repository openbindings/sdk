# OpenBindings evaluator test support

Run this finite suite against a custom `SchemaEvaluator` through the public document/context route:

```rust
use std::sync::Arc;
use openbindings_schema_evaluator_test_support::{run, Options};
let evaluator = Arc::new(openbindings_json_schema_evaluator::DefaultEvaluator::new());
let report = run(evaluator, &Options::without_unicode_property_matching());
assert!(report.is_success());
```

The package carries 496 groups and 1,566 cases from pinned upstream JSON Schema and Go SDK adversarial inputs. `fixtures/provenance.json` records their identities, exact optional capability cases, and two intentional translations: exact identifier matching and non-exhaustive diagnostic multiplicity. The original licenses accompany the fixtures. No sibling checkout or network access is needed.

The raw fixture remains unchanged. Its provenance also records one explicit
`runtime_policy_translations` entry for the current SDK admission policy:
`adversarial/resource-uri-names-another-id/0` now expects satisfaction from the unique
contained integer schema, preserving the historical conservative-refusal expectation
in the fixture. Native and browser qualification apply the same recorded translation.
This is an intentional supported-admission expansion, not a normative bug ruling.
The current default evaluator records 42 refusals across the unchanged 1,566 cases;
competing-carrier alias/pointer/direct routes have separate maintained regressions.

The report records preparation refusals separately from optional value outcomes. A preparation refusal does not manufacture a value-validation result. Permitted refusals are counted separately from validity verdicts. Options name exact cases with their predeclared capability reason. Unknown, blank, stale or unused declarations fail; a wrong Boolean verdict can never be exempted. Diagnostic checks verify instance paths and original schema locations. The kit itself has injected wrong-verdict, bogus-path and misclassified-cancellation controls.

`examples/boolean_evaluator.rs` is a small custom evaluator that consumes the original context and honestly declines non-boolean schemas. It demonstrates integration rather than claiming full capability. An integration test wraps the default evaluator with an original-context assertion and runs the complete suite.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Package publication and application cutover are separate actions.

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](../../docs/api-reference.md).

The kit checks explicit resource declarations: Complete must not return
ResourceUnavailable, and Incomplete must carry located ResourceUnavailable evidence.
The default trait declaration is Undeclared; the kit assigns no default-evaluator
completeness policy to an undeclared custom evaluator. All existing semantic and
refusal checks still apply.
