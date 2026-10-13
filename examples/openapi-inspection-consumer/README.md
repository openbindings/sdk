> Historical consumer, preserved for the previous inspection checkpoint. It is not a current adapter example and is not run by r1 qualification. Use [the fresh JSON-path consumer](../openapi-json-path-consumer).

# Independent OpenAPI inspection consumer

`src/lib.rs` uses the public SDK packages to inspect, select, author, assess and
reload an OBI. Tests independently create a binding and prove direct/referenced
mounted-target correspondence using the public OpenAPI client. The author owns
operation/source keys; unknown schema dialect and exact numeric content survive.

Run `python3 tools/verify-openapi-inspection.py --archives --wasm` from the SDK.
The tool generates a consumer manifest using extracted package dependencies and
the exact client Git revision. It does not alter this version-based SDK manifest.
`openbindings-internal-json` is used solely for qualification owner counters;
consumer behavior uses public SDK APIs. The small Wasm export is test plumbing,
not a proposed npm API. `examples/measure.rs` is the registered native benchmark.
