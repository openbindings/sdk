# OpenAPI JSON-path experiment consumer

A standalone package consumer with versioned SDK dependencies and an exactly pinned
independent OpenAPI client. The qualification tool supplies local archive patches;
they are test plumbing, not product path dependencies.

Run from the SDK root with the Rust/Wasm/Node tools available:

```sh
python3 tools/verify-openapi-json-path.py --archives --wasm
python3 tools/measure-openapi-json-path.py --edition 3.1.2 --output /absolute/fresh/31-evidence
python3 tools/measure-openapi-json-path.py --edition 3.0.4 --output /absolute/fresh/30-evidence
```

The qualifier runs native loopback HTTP, public consumer tests, serial logical-owner
checks and the TypeScript/Node exact-text Wasm bridge. JSON source and payload text
never passes through JavaScript Number. The Wasm host uses Rc state and an actual
pending JavaScript Promise; cancellation and abandonment drop the owned Rust host
future. Logical counters do not measure JS heap, resident memory or browser networking.

`src/lib.rs` illustrates existing inspection/synthesis/invocation facades. `call`
closes input and observes the authoritative terminal even after closed write admission.
It retains output until success. The panic/assert helpers around it are fixture setup,
not a suggested production error policy. The adapter's package README documents its
private r1 contracts and capability limits. Supply the kind explicitly to `author`
and `source`; the native and Node probes exercise 3.1.2 and 3.0.4 through the same
facades. Source edition and kind are independently checked by the adapter.

`examples/measure.rs` compares CPU work with the pinned client and core authoring,
using equal source/result content for synthesis. Stage rows are descriptive diagnostics;
registered end-to-end budgets are evaluated on paired observations. No measurements
here establish whole-network HTTP parity or production browser/Worker performance.
