# Ordinary JSON CRUD: qualified implementation

The bounded CRUD slice passes local qualification at implementation commit
`0d66be7bde5d0a63df228d19cb85295875c8f11e`. [PR #11](https://github.com/openbindings/sdk/pull/11)
contains the implementation and review evidence; this report does not claim it is
merged or published. The adjacent [Rust role closeout](../../role-contract-closeout.md)
found no material contract gap requiring a redesign before this adapter expansion.

## Result

The existing private OpenAPI 3.0 and 3.1 kinds support GET, POST, PUT, PATCH and
DELETE for the finite JSON profile. A consumer can inspect a service, synthesize
an OpenBindings document, save/reload it and invoke its operations. Independently
authored documents exercise the same public binding and operation invokers.

Response selection observes exact status before range before default. A selected
content-less success requires complete empty delivery and produces no output;
JSON null remains one output. HTTP 204/205 require empty delivery. The 3.0/3.1
DELETE request-body difference follows the independent client's edition handling.
Unexpected bytes and incomplete delivery fail after dispatch without promising
rollback or retry. Existing non-2xx results remain status-only errors.

This adds adapter capability without changing core, generic role APIs, published
interfaces or the independent client. The client remains pinned to `fe09522`.
[Portability observations](portability.md) distinguish candidate binding meanings
from upstream semantics and our implementation policies. Old binding drafts were
excluded. No new public binding specification is claimed complete.

## Qualification

Run from the SDK checkout with the documented Rust/Wasm/Node tools:

```sh
python3 tools/verify-openapi-json-path.py --archives --wasm
```

The [receipt](qualification/receipt.json) records 29 successful commands, 92 input
hashes and 12 Cargo archive hashes. All recorded input hashes were rechecked after
completion. Its `dirty: true` reflects only the then-untracked Astra review record;
the [status log](qualification/status.log) preserves that distinction.

- **138 affected Rust tests/doctests passed**, including five focused CRUD tests,
  seven native HTTP/manual tests and existing role/adapter regression coverage.
- **Three extracted-package consumer tests passed.** The CRUD test performs 48
  literal exchanges across two editions, manual/synthesized documents, both
  invokers and six operations. The five manual source/document/exchange fixtures
  still match their [pre-implementation hashes](manual-freeze.json).
- Native loopback HTTP covers PUT, PATCH and DELETE with exact JSON and empty
  202/204/205 responses. Negative cases cover response selection, missing/null
  bodies, invalid JSON, unexpected bytes, incomplete delivery, cancellation,
  unsupported neighbors and analysis limits.
- Strict product/library/test Clippy and ordinary-example Clippy passed. Only the
  unchanged, stopped `auth_resolution` example has two named style allowances;
  it is compiled and linted, never executed. Formatting and diff checks passed.
- TypeScript compilation and the local async Node/Wasm probe passed. The probe
  runs the 48-exchange CRUD suite twice, then retains the existing authentication
  and 1,000-cycle-per-kind cleanup checks. Owner counts were `0,0 -> 8,0 -> 8,0`
  (SDK, client). The eight SDK owners are the process-static embedded schemas.
  This establishes stable ownership across the repeated suite, not zero retention,
  browser-network qualification or a released TypeScript role API.
- Package resolution uses extracted SDK archives and the exact client git pin;
  the core dependency tree contains neither adapter nor client. CI now maintains
  the external consumer on native platforms and in the Node/Wasm job.

The initial focused tests failed on the baseline as recorded in
[baseline-red.log](baseline-red.log). A new cold-process owner assertion was
corrected to record cold retention and compare two completed, warmed suites;
runtime behavior was unchanged by that test correction.

## Review and limits

[GPT-6 Astra xhigh](astra-review.md) independently reviewed the role foundations
and CRUD implementation, ran the initial four focused tests, checked fixture
hashes, and verified subsequent targeted fixes. [Claude Opus 5.5 Extra](claude-review.md)
reviewed runtime decisions and documentation. Both reviews closed material
findings. Their exact scopes are recorded; Claude did not run the qualification.

**AJ20 remains unresolved and deferred, with budgets unchanged.** No benchmark,
calibration or performance grading was resumed. The bounded response-selection
metadata is an implementation choice, not a fresh measured-performance claim.
See the existing [performance deferral](../openapi-auth-json/performance-follow-up.md).

This slice stops here. Rich failure outcomes and synthesis fidelity are separate
next assignments. HEAD/OPTIONS/TRACE, richer parameter encodings, forms, alternate
body media, response metadata and first-class TypeScript role APIs remain outside
the qualified profile. Private labels remain experimental, and the HTTP 205
correction is explicitly not strict compatibility with the prior invalid behavior.
