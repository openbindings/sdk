# Private OpenAPI r1 adapter checkpoint

The adapter now implements the existing inspection, synthesis and binding invocation
roles. Generic operation invocation remains in the existing SDK companion. This work
uses `local.openapi-json-probe.r1`, a private experiment; it creates no binding
specification and changes neither core nor published role interfaces.

**This is a bounded checkpoint with disclosed gaps, not universal OpenAPI support.**
Existing binding drafts and their derivative expectations were excluded. The manual
consumer contract and fixtures were frozen before synthesis existed. The independent
HTTP oracle checks literal method/path/query/entity and caller results; generated
names and synthesized output are not the execution oracle.

## Results

- 24 fresh adapter tests and 121 affected tests/doctests pass. Native HTTP covers
  manually authored and synthesized/reloaded GET and POST, exact large integers,
  Unicode bodies, source-relative references, two mounts, redirects and cancellation
  after server receipt. Native Send and local Rc provider ownership are exercised.
- Cargo archives, exact client Git pin, core dependency isolation, strict Clippy,
  external consumer, and the smaller invocation-only dependency check pass.
- Native and Node/Wasm lifecycle mixes each run 1,000 cycles including success,
  context refusal, malformed response, cancellation and abandonment. Measured SDK
  and client arena owners return to a warmed baseline. The Node baseline is 8/0;
  the host crosses an actual JavaScript await boundary. This does not measure JS
  heap, allocator peaks, RSS or browser/Worker networking.
- All 22 registered CPU rows pass in three fresh alternating process pairs with
  seven >=50ms samples each. No retries, failed gates or inconclusive rows occurred.
  Thirty stage rows report independent diagnostics, not additive components.

Representative warm binding medians on this host:

| Workload | Direct client | Adapter binding | Added CPU |
|---|---:|---:|---:|
| GET, 1 KiB response | 5.02 µs | 8.78 µs | 3.76 µs |
| POST, 1 KiB request/response | 5.66 µs | 12.22 µs | 6.56 µs |
| GET, 64 KiB response | 159.71 µs | 207.96 µs | 48.25 µs |
| POST, 64 KiB request/response | 159.43 µs | 356.47 µs | 197.03 µs |

These are CPU comparisons using a programmable host and complete decoding, not
whole-network HTTP parity. Passing uses **every matched pair's ceiling**, a stricter
rule than checking only the aggregate medians. `ceiling_ns` in the summary shows the
aggregate-median ceiling for orientation; raw per-process baselines and paired
ratios/overheads remain available. Schema validation has separate matched rows.

The adapter bridges separate exact-value arenas: one client parse per supplied
scalar/body and one SDK parse for a decoded successful response. The client also
parses its response. These copy/retention bounds are structural evidence; the stage
timings and owner cleanup are measured. They are not measured allocator-peak counts.
The provider accepts one request value and emits at most one output in r1; applications
can retain arbitrary returned values. No zero-copy or process-memory ceiling is claimed.

## Reviews and repairs

[Claude Opus 5.5 Extra](claude-implementation.md) supplied an independent code review.
Two separate GPT-6 Astra xhigh reviewers examined contracts/effects and consumer/
resource behavior, then independently checked the focused repairs. The
[reconciliation](implementation-reconciliation.md) distinguishes adopted findings,
client-owned checks and implementation policy. The [Astra record](astra-implementation-and-verification.md)
records executed versus static evidence.

Repairs cover exact integrality, aggregate retained metadata budgets, early parameter
limits, terminal-preserving consumer helpers, truthful response/ambiguous-reference
coverage, scoped opaque context targets and failure-output tests. No new generic
role trait or internal-behavior rule was added to a published interface.

Claude's initial decision and implementation reviews are complete. A further Claude
repair recheck could not be dispatched because computer use reported a locked Mac;
final repair verification is Astra's, not an assertion of fresh Claude concurrence.

## Explicit remaining boundaries

The pinned client refuses literal Unicode path templates (`InvalidDestination`). A
minimal direct-client reproducer is maintained. Encoded Unicode/tilde paths pass,
but that does not erase the original literal-Unicode expectation. This limitation
is disclosed in coverage; it is not elevated into a future binding prohibition.

Non-204 no-content success declarations, forms, auth acquisition, streaming, richer
parameter locations, ambiguous Path Item merges, general schema translation and
additional OpenAPI version families remain outside the implemented profile. Synthesis
reports projection loss or unsupported/unknown inventory rather than full fidelity.
Browser/Worker transport and a published first-class TypeScript API remain separate.

## Evidence and replay

- [Checkpoint summary](summary.json), [final package receipt](qualification-receipt.json).
- [CPU summary](performance-summary.json), [raw observations](performance-raw.json),
  [exact timing build's receipt](performance-qualification-receipt.json).
- [Frozen experiment](experiment-contract.json), [manual freeze](manual-freeze.json).
- [Nine portability observations](portability-observations.json), all provisional.

The final archive differs from the timing archive only in two test files that add
fresh consumer coverage. Runtime adapter code, manifests and measurement source are
identical; both snapshots and hashes are retained. Paths in receipts are host-specific.
The pre-existing vendored serde deprecation warning is recorded and is not a new
adapter warning.

```sh
python3 tools/verify-openapi-json-path.py --archives --wasm
python3 tools/verify-openapi-invocation-only.py
python3 tools/measure-openapi-json-path.py --output /absolute/new/evidence-directory
```

No merge, publication, automatic acquisition or external operation was performed.
