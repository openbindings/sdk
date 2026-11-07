# Pinned qualification fixtures

The current specification corpus is copied from revision c1419c9fc3ae5a00c2b83da710c11bf67dab7934: 372 document cases and 133 scenarios (505 total). The public-API observer and independent Python judge preserve exact text/base64 carriage and include deliberately wrong-result controls. Generated observations are ignored. The evaluator kit still carries its historical 1,566-case source; the migration adjudication assigns 1,546 value cases and 20 document-admission cases. Applying that adjudication to the evaluator runner is separate from this document-admission checkpoint.

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
cargo run --locked -p openbindings-json-schema-evaluator --example replay
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

The original matrix's 1,086 cases and 1,262 concrete-completion cases remain
unchanged. `reference-current-oracle.json` records their current obligations:
194 original admission negatives, 2,072 preserved/adjudicated S/F values,
50 independently adjudicated former semantic gaps, 30 genuinely unavailable
external-content cases, and two explicit-budget controls. Concrete replacement
and ordering rows use their concrete S/F expectations, not inherited undefined
labels. The oracle is not generated from current evaluator observations.

Known local defects are evaluated per value; preparation failure is not used as
an undefined verdict. Available annotation-producing applicators remain coupled
with validity, and missing content is not assigned an invented source origin.
`semantic-soundness.json` and `semantic-limits.json` retain independent review
counterexamples alongside the unchanged corpus. Rust/TypeScript parity is a
separate gate and requires a freshly built Wasm artifact.
