# OpenBindings evaluator test support

Qualify a schema evaluator through the public document and contextual schema
workflow. The current pinned suite contains 1,546 value cases in 486 groups.
Every value is checked and explained. Failure explanations are also exercised
under cancellation and as a value nested inside a larger exact input.

```rust
use openbindings_schema_evaluator_test_support::run;
let evaluator = openbindings_json_schema_evaluator::DefaultEvaluator::new();
let report = run(&evaluator);
// A release gate requires report.is_success(). Inspect individual observations
// while implementing missing capabilities; refusals remain qualification gaps.
for observation in &report.observations {
    if !observation.failures.is_empty() {
        eprintln!("{}: {:?}", observation.id, observation.failures);
    }
}
```

A successful report establishes every finite expected semantic answer. It does
not prove complete JSON Schema support. Unsupported capabilities, unavailable
resources and other no-verdict results remain recorded separately from wrong
verdicts; none can be exempted to make this suite pass. An established undefined
result is a semantic answer, distinct from declining to establish it.

Diagnostics may be incomplete, but every reported location must exist in the
original instance or schema resource. Instance pointers start at the value passed.
An interrupted explanation cannot undo a failure already established, and cheap
checks must agree with explanations without manufacturing detailed problems.

All 1,566 historical case identities are accounted for in the packaged ledger:
1,546 remain value cases and 20 are exact negative document-admission witnesses.
Four retained supplied-resource reexpressions preserve their original documents
as OBI-09 negatives. Original inputs, licenses, current inputs, dispositions and
content hashes are packaged together. Qualification needs no network or sibling
checkout; the historical inputs are evidence, not implicit runtime translations.

Use `run` and `Report` for the ordinary workflow. `suite`, `judge`, `judge_detail`
and `judge_interruptions` support independent harnesses and controls. The
[custom evaluator guide](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md)
describes the original document and resource context supplied to an evaluator.
This unpublished package requires Rust 1.88.
