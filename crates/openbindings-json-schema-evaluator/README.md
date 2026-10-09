# OpenBindings JSON Schema evaluator

This optional companion evaluates OpenBindings operation contracts using an explicit immutable resource context. It supports JSON Schema 2020-12 and exact JSON numbers. It never acquires HTTP or filesystem resources.

```rust
use openbindings::{ParsedDocument, ResourceSet, Side, JsonValue, ValueOutcome, ContractPreparation};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

let document = ParsedDocument::parse(
    br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
)?;
let context = document.value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())?;
let ContractPreparation::Ready(input) = context.prepare("lookup", Side::Input) else {
    return Err("input contract could not be prepared".into());
};
assert!(matches!(input.validate(&JsonValue::string("item")?), ValueOutcome::Satisfies));
# Ok::<(), Box<dyn std::error::Error>>(())
```

A prepared contract retains its resources after the document/context is dropped. Preparation reuses deterministic results within a context. The context retains four most-recent prepared entries by default; `ValueContractOptions.cache_capacity` controls this count, including zero to disable caching. Caller-retained contracts survive eviction; this is an entry cap, not a byte cap. Concurrent first calls may perform duplicate preparation before one result is retained. Cancellation and transient evaluator failures are not cached. Different contexts never share resource identities or compiled contracts.

`NoVerdict` distinguishes unsupported capabilities, conservative preparation, missing resources, limits and cancellation. A missing reference or invalid pattern in a potentially visited branch may cause a conservative refusal even if another branch would establish validity. Such a refusal does not claim semantic undefinedness. Unicode property-escape *syntax* follows ECMA-262 edition 11; actual property matching is explicitly unsupported and declined only when applied. `format` remains an annotation. Custom/historical dialects are not interpreted as 2020-12.

Mismatches report actual instance locations and original schema locations where available. They do not promise every possible failing keyword or Go's diagnostic multiplicity. `problems_complete` means the selected diagnostic pass completed without truncation, not exhaustive traversal of every semantically redundant failure. Diagnostic collection has its own work scope and allocation cap; a confirmed mismatch remains a mismatch when its diagnostics are truncated. Messages omit instance values.

`Limits` bounds evaluation work, recursion, regex work, diagnostic output, dependency compilation depth and pattern admission. Parsing, graph preparation and dependency compilation use separate admission bounds. Cancellation is cooperative; dependency compilation/evaluation has bounded regions that are not preempted midway. Browser applications should use a Worker for large synchronous jobs.

The vendored adaptation is an implementation detail. Upgrade it with the recorded patch invariants and the complete evaluator contract suite; a dependency draft label alone is insufficient evidence.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Package publication and application cutover are separate actions.
