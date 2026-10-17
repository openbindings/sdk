# OpenBindings evaluator test support

Check a `SchemaEvaluator` implementation through the same document and value
contract route used by applications. The suite reports preparation refusals,
value verdicts and diagnostic-location errors separately.

```rust
use openbindings_schema_evaluator_test_support::{run, Options};
let evaluator = openbindings_json_schema_evaluator::DefaultEvaluator::new();
let report = run(&evaluator, &Options::without_unicode_property_matching());
assert!(report.is_success());
```

Replace the evaluator in this example with your implementation. Start with the
[boolean evaluator example](https://github.com/openbindings/sdk/blob/main/crates/openbindings-schema-evaluator-test-support/examples/boolean_evaluator.rs)
for a small implementation that honestly refuses non-boolean schemas. See
[custom evaluator integration](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md)
for the original-document and resource contract.

## Limits

This is a finite qualification suite, not proof of complete JSON Schema support.
A successful report may contain explicitly permitted capability refusals.
`Options` declares exact cases and reasons; a wrong verdict cannot be exempted.
Unknown or unused declarations fail. Incomplete diagnostics are permitted, but
reported paths must exist in the original instance or schema resource.

Fixtures and licenses are packaged, so the suite needs neither network access
nor another checkout. Fixture provenance and policy translations are maintained
with the suite. An evaluator's own limits and resource policy remain its authors'
responsibility. This unpublished package requires Rust 1.88.

## Reference

Use `run`, `Options` and `Report` for the ordinary workflow. `suite` and `judge`
support host-specific harnesses. The
[maintainer guide](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
links fixture provenance and implementation checks.
