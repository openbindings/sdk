# OpenBindings evaluator test support

Check a `SchemaEvaluator` implementation through the same document and value
contract route used by applications. Every value is both checked and explained,
and every failing value is explained again under cancellation after increasing
numbers of control observations: an interrupted explanation may answer no
verdict before its verdict, but never another verdict, and never undoes a
failure it has established. The suite reports preparation refusals, value
verdicts, disagreement between the calls and diagnostic-location errors
separately.

```rust
use openbindings_schema_evaluator_test_support::{run, Options};
let evaluator = openbindings_json_schema_evaluator::DefaultEvaluator::new();
// The default evaluator's qualified limitations. It decides the two cases of
// those capabilities whose values never reach a property escape.
let mut options = Options::without(&[
    "unicode-property-matching",
    "backtracking-pattern-matching",
    "undefined-results",
]);
options.permitted_refusals.retain(|id, _| {
    !matches!(
        id.as_str(),
        "adversarial/lazy-pattern-properties/0" | "adversarial/lazy-pattern-properties/1"
    )
});
let report = run(&evaluator, &options);
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
`Options::without` permits, case by case, the stated refusal of each case that
declares one of the suite's optional capabilities: `unicode-property-matching`
(Unicode property escapes in patterns), `backtracking-pattern-matching`
(patterns with lookaround or backreferences) and
`undefined-results` (establishing that a value's validity is undefined).
A case that reaches a schema in another dialect, such as draft 2019-09 or a custom
meta-schema's vocabularies, expects JSON Schema's verdict for it and also accepts
an unsupported-capability refusal without a declaration. Where JSON Schema leaves
the assertion of `format` to the implementation, as under a format-assertion
vocabulary declared false, the case expects the annotation reading's verdict,
since OpenBindings treats `format` as an annotation.
Unknown or unused declarations fail. Incomplete diagnostics are permitted, but
reported paths must exist in the original instance or schema resource. Instance
pointers start at the value passed: explaining a failing value inside a larger
text must not change its problems.

Fixtures and licenses are packaged, so the suite needs neither network access
nor another checkout. Fixture provenance and policy translations are maintained
with the suite. An evaluator's own limits and resource policy remain its authors'
responsibility. This unpublished package requires Rust 1.88.

## Reference

Use `run`, `Options` and `Report` for the ordinary workflow. `suite`, `judge`,
`judge_detail` and `judge_interruptions` support host-specific harnesses. The
[maintainer guide](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
links fixture provenance and implementation checks.
