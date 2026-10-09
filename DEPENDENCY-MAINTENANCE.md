# Internal dependency maintenance contract

The SDK carries three renamed internal packages. They are implementation details, not supported extension APIs. Their upstream versions and complete crate checksums are recorded by `tools/dependency-patches.py`; that script compares every file with the original downloaded crate archive and emits a full diff plus before/after digests. The original upstream Git identities are retained as `UPSTREAM-VCS.json`; original manifests are retained as `UPSTREAM-Cargo.toml`. Cargo-reserved generated filenames were renamed so the internal packages can themselves be archived. Cargo extraction completion stamps are removed. Original upstream lockfiles remain provenance; the SDK workspace lock is the development/build authority.

| Upstream | Applied package | Upstream revision |
| --- | --- | --- |
| jsonschema 0.58.6 | openbindings-internal-jsonschema 0.58.6-ob.1 | 55ac1664384793c5d3b20745d3f9c169dfe2c06f |
| jsonschema-value 0.58.6 | openbindings-internal-jsonschema-value 0.58.6-ob.1 | 55ac1664384793c5d3b20745d3f9c169dfe2c06f |
| regress 0.12.0 | openbindings-internal-regress 0.12.0-ob.1 | See its retained upstream VCS record and archive checksum |

## jsonschema

- `Cargo.toml`: unique private package identity and explicit sibling package dependencies prevent accidental resolution to an unpatched evaluator/value/regex implementation. Public crate names and upstream licenses are retained.
- `src/lib.rs`: exposes internal budget/regex support needed by the adapter.
- `src/ob_work.rs`, `src/node.rs`, `src/validator.rs`, `src/compiler.rs`: scoped evaluation steps, depth, recursion/cycle handling and diagnostic accumulation. Guards restore thread-local state after return, nested exhaustion and unwinding. Arithmetic exhaustion is separate from false validity. Entry and primitive traversal limits are not advertised as nanosecond deadlines.
- `src/ob_ecma.rs`, `src/regex.rs`, `src/canonical/context.rs`: route qualified dynamic patterns through the ECMAScript interpreter, preserve pattern preparation failures, and distinguish selected unsupported Unicode-property matching from work exhaustion.
- `src/error.rs`, `src/keywords/custom.rs`: retain lazy exact instance views through custom-keyword error context; avoid recursive materialization/drop of 10,000-level literals. Internal diagnostics expose bounded metadata and original locations, not whole instance values.
- `src/keywords/additional_properties.rs`, `any_of.rs`, `helpers.rs`, `items.rs`, `min_length.rs`, `properties.rs`, `property_names.rs`, `required.rs`, `unevaluated_items.rs`, `unevaluated_properties.rs`, and `src/properties.rs`: preserve generic flat-value operation, scoped evaluation/diagnostic work, ECMAScript matching and dynamic reference behavior in the affected applicator/keyword paths. Check the full diff when rebasing; these are evaluator-sensitive changes, not mechanical wrappers.

## jsonschema-value

- `Cargo.toml`: private package identity and bounded-integer arithmetic dependencies.
- `src/lib.rs`, `src/types.rs`, `src/cmp.rs`: exact decimal hooks for value representations; equality, ordering and hashing must agree across numerically equivalent spellings.
- `src/ob_decimal.rs`: normalized coefficient plus signed decimal exponent. Exponent parsing, comparison and small offsets avoid constructing an enormous integer/power. Expensive divisibility uses bounded coefficients/powers and returns explicit non-admission. Zero, negative zero, signed exponents, non-integral divisors, normalization shifts, and equivalent spellings must remain correct.
- `LICENSE`: restored from the exact upstream Git revision because the published jsonschema-value archive omits the standalone license text. The upstream MIT grant and copyright are retained.

## regress

- `Cargo.toml`, `src/lib.rs`: private identity and internal modules; the SDK explicitly selects the classical interpreter required by the work budget.
- `src/api.rs`, `src/classicalbacktrack.rs`, `src/ob_budget.rs`: finite interpreter work and cancellation propagation; a failed/exhausted match must never become an ordinary non-match that supports a wrong validation verdict.
- `src/parse.rs`, `src/ob_ecma11_properties.rs`: ECMAScript 11 property-name recognition and bounded pattern expansion/admission. Supported syntax and selected matching capability are distinct. Unicode property matching remains an explicitly qualified refusal only when selected by evaluation; irrelevant branches retain existing verdicts.

`OPENBINDINGS-PATCH.md` in each package identifies it as modified. These records and the full file diffs are the patch inventory; the line count alone is not evidence that the maintenance burden is small.

## Invariants and regression gates

Keep exact input in the SDK's flat immutable arena. Do not convert values through f64 or serde_json::Value as an authoritative convenience path. The private compiler projection can strip annotations only because the declared evaluator is a validity evaluator with format assertion disabled; original SDK documents and diagnostics preserve original content. The adapter's const/enum and numeric keywords preserve exact values and charge work.

Core resources are explicit. Both the registry and evaluator use a denying retriever. A host enabling dependency HTTP/file features must still fail to produce any HTTP/FIFO trap hit through the SDK; direct dependency positive controls prove the traps work.

Before adopting an upstream update:

1. Archive/checksum the new upstream sources and record its Git revision. Compare every old patch against the new API and behavior; never copy a Cargo registry cache over the production fork.
2. Prefer an equivalent upstream hook and remove a patch when original-value, scope, work and diagnostic invariants remain provable. Keep unique private names until the unmodified upstream can satisfy them.
3. Rebase the change in isolation. Re-run the full pinned 435-case OpenBindings corpus, 1,566 evaluator cases and exact-case refusal ledger on native and real Wasm. Run the affected upstream library tests, negative controls, no-I/O feature-unification traps, dynamic-scope/isolation, cancellation, deep literals, exact arithmetic and ownership probes.
4. Repeat the six finite fuzz campaigns, frozen representative performance/memory series and packed consumer tests. A verdict becoming a refusal is a regression unless the governing authority intentionally changes; update scope only through an explicit separate decision.
5. Refresh RustSec and license inventories. Retain missing upstream notices from exact source revisions. Document all intentional translations, APIs removed from the fork, failed attempts and new limits.
6. Regenerate the full patch manifests, source archive and package hashes only after review; do not silently rewrite the pinned baseline.

The complete diff inventory is in `docs/dependency-patches/`. Reproduce it with
`python3 tools/dependency-patches.py --upstream-dir <original-crate-archives> --output <new-directory>`;
verify the original archives against the manifest checksums first.

The upstream regress suite and jsonschema-value library/numeric suite pass in the
SDK-relevant configuration. The jsonschema library suite has 17 exact dispositions:
two Draft 4/non-u regex cases, four finite huge-number integer cases, three legacy
non-u character-class forms, one previous regex-engine failure expectation, three
alternate Rust-regex-backend whitespace cases and four Unicode-property raw-API
cases. They are not SDK correctness exemptions. The supported SDK path has explicit
public regression witnesses for its ECMA u behavior, exact integers, valid empty
match and guarded unsupported-property outcome. The upstream crate omitted a
referenced draft7 fixture; replay supplies that file from the separately pinned
JSON Schema Test Suite. Full names, resolved upstream-test locks, original failures
and rerun receipts are preserved in the migration evidence. A change to this exact
failure set requires investigation, not a broader skip filter.

These forks are private implementation dependencies. Their unguarded alternate
backends, legacy drafts and old engine-failure behavior are not supported SDK APIs.
The 435-case normative corpus, 1,566-case evaluator kit, explicit public regression
witnesses and no-I/O controls govern the supported product behavior.

## Precise witness attribution (SDK-A01)

[The per-test mapping](docs/upstream-disposition-witnesses.json) supplements the
historical 17-entry disposition ledger without changing old receipts. Two Draft4
escaped-dash tests are excluded by the supported 2020-12/ECMA-u scope; they have
no claimed direct public test. The ordinary huge-number type, legacy class,
empty-match and whitespace assertions are named directly. Huge-number `items`
wrappers share the exact numeric predicate but are not exercised by the root-type
witness itself. The Unicode-property witness directly covers the selected matched
`patternProperties` shape; `$ref`, `allOf` and `then` variants additionally rely
on the documented enclosing guard/source reasoning. That single test does not
claim four distinct wrapper executions or the entire raw test's unmatched branch.
No new Draft4 capability or fresh upstream-suite run is claimed in the facade repair.
