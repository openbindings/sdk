# Pinned qualification fixtures

The specification corpus is copied from revision 30416ef1ca626e58eb8c383a0c71422a68f9243f. The immutable inventory contains 440 cases. The observer uses public SDK APIs; the Python judge is independent and includes deliberately incorrect-result controls. These files support repository CI and replay, not runtime SDK features. Generated observations are ignored. The reusable evaluator kit separately preserves 1,566 case IDs with its own provenance: 1,564 evaluator observations and two core rejection cases.

For a specification re-pin, replace only the files already present in
`inputs/spec/` with the same paths from the selected checkout. Rebuild
`inputs/blueprint/case-inventory.json` from `conformance/manifest.json`: for each
`files` entry enumerate `path#/tests/index`; for each `scenarioFiles` entry collect
its scenario `id` values. Sort the combined IDs, check uniqueness and the manifest
counts, and update `spec_commit`, `count` and this README's revision/count sentence.
`replay/corpus.py` independently reconciles that inventory during replay. Verify
the snapshot with `node tools/verify-spec.mjs . SPEC_CHECKOUT FULL_REVISION --self-test`.

The inputs directory contains only the pinned specification snapshot and inventory,
plus the independent container-finding regression cases. Historical migration
ledgers and one-off receipts are maintained outside the product. The evaluator
kit in `crates/openbindings-schema-evaluator-test-support/fixtures` contains the
frozen raw cases and the admission/expectation policies consumed by native and
browser replay. These policies remain live contract inputs.

Check and explain every case through the public evaluator API:

```sh
cargo run --locked -p openbindings-schema-evaluator-test-support --example replay
```

The live allocation thresholds and generators are described in
`../diagnostic-memory-probe/README.md`.

A listener-free TypeScript corpus lane uses the shipped facade:

```sh
node tools/qualification/replay/node-corpus.mjs tools/qualification/corpus/full-requests.jsonl > target/document-diagnostics/ts-corpus.jsonl
python3 tools/qualification/replay/check-corpus-parity.py --native PATH_TO_NATIVE_JSONL --typescript target/document-diagnostics/ts-corpus.jsonl
```

Parity includes all findings and source coordinates. The comparison normalizes
Rust/TypeScript field spelling and excludes only the TypeScript observer's extra
setup tags and the native observer's truncation field; shared cap tests cover the
latter in Node and the installed package.


Independent container failures have four additional public regression cases in
`inputs/container-findings-cases.json`, outside the unchanged specification corpus
inventory. `replay/check-container-findings.py --binary PATH` replays them through
native and TypeScript APIs and compares complete findings, including coordinates.
These cases check complete container findings without changing the frozen corpus.

`static_reference_defects.rs` and `static-reference-defects.test.mjs` read the
same packaged evaluator fixtures (`tests/fixtures/reference-{matrix,completions}.json`).
Set `REFERENCE_RECEIPT` to separate output paths when running each, then run:

```sh
python3 tools/qualification/replay/check-reference-completions.py native.json typescript.json
```

The accepted matrix's 1,086 cases are retained, including the unsupported
families; 36 static-reference observations change. Additional controls preserve
siblings and identities while replacing individual defective invocations with
concrete schemas. Every migrated kit/corpus case has opposite concrete repairs.
The complete-fragment oracle, completion comparisons, and injected false results
run without network or an independent resolver implementation.
