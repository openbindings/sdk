# Implementation review reconciliation

Two independent GPT-6 Astra xhigh reviewers plus Claude Opus 5.5 Extra reviewed the fresh packet. Historical drafts and assertion sets were excluded.

## Astra
- Exact integral decimal/exponent inputs: fixed trailing-zero accounting across decimal separator. Public binding and generic operation/evaluator regression passes, including neighboring fractions.
- Aggregate descriptor retention: retain only necessary server/parameter metadata; charge its sizes, plus schema and inventory, to the result budget. Scaled inherited metadata regression passes. This is logical accounting, not allocator/RSS measurement.
- Late parameter count check: precharge count with cancellation checkpoints, then indexed lookup/conversion. No public role change.
- Consumer helper: preserve authoritative terminal after a closed write; check cardinality after evaluating the item. Missing-operation regression now passes.
- Manual mount and Unicode route gaps: mounted path passes. The literal Unicode source path failed. Percent-encoded Unicode path passes; literal Unicode is now disclosed as unsupported, with the original unmet acceptance expectation retained. No rewriting of source keys or a second HTTP engine.

## Claude
1. Accepted: explicit successful no-content responses other than 204 are unsupported before effects in r1, rather than synthesized as supported and then failing invariably. Runtime unexpected empty successful deliveries still fail. A later mapping expansion needs a new experiment revision.
2. Accepted: non-2xx media/header declarations do not prevent status-only invocation. Lost representation is disclosed in projection coverage. Successful media/header profile limits remain.
3. Accepted with distinction: overlapping effective Path Item method/parameters/servers are unsupported; skipped unestablished units make inventory non-exhaustive. No upstream-invalid or binding-excluded claim is inferred.
4. Verified existing dependency policy: pinned reqwest companion Client::new explicitly configures redirect::Policy::none. Added real loopback redirect test and post-receipt cancellation test: one POST receipt, no follow-up, correct terminal.
5. Verified delegation: required path parameter/body preparation belongs to the client. Maintained no-evaluator regression covers required parameter refusal. A dedicated required-body and parameter-count regression now passes; no second required-field engine is introduced.
6. Accepted concern, different repair: target is an opaque encoded tuple of experiment, optional retrieval identity, path, method and sorted server set. Retrieval URI alone would still collide across differing operation choices. No durable/persistence semantics or trusted cache-key promise. A temporary omission was rejected by the existing required role shape and immediately repaired.
7. Accepted as this implementation's admission policy: duplicate names anywhere in invocation input refuse before effects. Do not promote this choice to a universal binding specification or generic role rule. Exact number text remains preserved.
8. Accepted: negative response helper explicitly asserts no preceding successful outputs. Text/plain JSON-looking bytes fail. Client Response::json verifies media/completeness; adapter additionally requires explicitly complete header delivery.

No published role, core SDK API, client source, or binding specification was changed. Product claims remain private, bounded, and provisional. Final archive, expanded native/NodeWasm lifecycle mix, and minimal invocation-only qualification pass. All 22 registered CPU gates pass; 30 diagnostic stage rows are also retained. Literal Unicode and measured allocation-peak qualification remain disclosed limits.
