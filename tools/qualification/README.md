# Pinned qualification fixtures

The specification corpus is copied from revision 2f7d754dc2da374058cd517064c17e50f7d95d99. The immutable inventory contains 435 cases. The observer uses public SDK APIs; the Python judge is independent and includes deliberately incorrect-result controls. These files support repository CI and replay, not runtime SDK features. Generated observations are ignored. The reusable evaluator kit separately preserves 1,566 case IDs with its own provenance: 1,564 evaluator observations and two core rejection cases.

S6c's [case-ID ledger](inputs/s6c-migration-ledger.json) retains all 1,566 evaluator
IDs and all 435 corpus IDs, with original input hashes, exact before/after
observations, refusal stages, source coordinates and explicit partition changes.
To check a fresh replay against it, run the test-support `replay` example and pass
its JSON plus the production corpus JSONL to `replay/check-s6c-migrations.py`:

```sh
cargo run --locked -p openbindings-schema-evaluator-test-support --example replay > kit.json
python3 tools/qualification/replay/check-s6c-migrations.py --kit-receipt kit.json --corpus-receipt path/to/full-actual.jsonl
```

The nine supplied-resource migrations retain refusal reason/code and change only
source location. `adversarial/unreached-uncopied-targets/0` (formerly satisfies) and
`/1` (formerly fails) are now core OBI-12 rejection regressions. No raw fixture seam
survives, because the nine re-expressions preserve their outcomes.

The diagnostics migration records diagnostic-only changes for every affected case in
[`diagnostics-migration-ledger.json`](inputs/diagnostics-migration-ledger.json). The new
`replay/check-diagnostics-migrations.py` compares exact before/after receipts, including
both default Coarse and explicit Upstream kit runs (`--example replay -- --upstream`).
It fails on any verdict, refusal stage/reason/code, inventory, or rule-evidence drift.
Diagnostic completeness no longer serves as verdict evidence in either native or
browser judges; malformed complete diagnostics and bogus locations still fail.


Document location expectations separately account for the probe's 19 changed
locations: five array-element fixes and fourteen intentionally localized OBI-10
nodes. See `inputs/document-location-expectations.json`. The memory qualification
and raw numbers are documented in `../diagnostic-memory-probe/README.md`.

A listener-free TypeScript corpus lane uses the shipped facade:

```sh
node tools/qualification/replay/node-corpus.mjs tools/qualification/corpus/full-requests.jsonl > target/document-diagnostics/ts-corpus.jsonl
python3 tools/qualification/replay/check-corpus-parity.py --native PATH_TO_NATIVE_JSONL --typescript target/document-diagnostics/ts-corpus.jsonl
```

Parity includes all findings and source coordinates. The comparison normalizes
Rust/TypeScript field spelling and excludes only the TypeScript observer's extra
setup tags and the native observer's truncation field; shared cap tests cover the
latter in Node and the installed package.
