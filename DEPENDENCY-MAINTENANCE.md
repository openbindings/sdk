# Internal dependency maintenance contract

The SDK carries two renamed forks: jsonschema 0.58.6 and regress 0.12.0.
Both are private implementation dependencies. jsonschema-value 0.58.6,
referencing 0.58.6 and serde_json 1.0.151 now use their unmodified registry
identities. `docs/dependency-isolation.json` records the three upstream archive
checksums and complete Rust source digests. `tools/verify-dependency-isolation.py`
checks those sources, the single-serde graph, absence of SDK-requested arbitrary
precision, retained fork identities, and the cancellation/schema boundaries.
Its optional `--upstream-dir` additionally checks the downloaded archives.

`tools/dependency-patches.py` compares every retained fork file with its exact
upstream crate archive, emitting applicable patches and before/after hashes.
Original manifests and Git identities remain `UPSTREAM-Cargo.toml` and
`UPSTREAM-VCS.json`; licenses and publish guards remain. Retained upstream locks
are provenance; the SDK workspace lock is the build authority.
Refresh both attribution inventories and notices offline with
`python3 tools/attribution.py`; verify with `--check`. The upstream value crate's
omitted license is retained as a hash-verified supplement from the same pinned
upstream revision, not dropped when its fork disappears.

## Advisory policy

The required `advisories` job in [CI](.github/workflows/ci.yml) runs
`cargo deny --workspace --all-features --locked check advisories` on pull requests
and pushes to `main`. It checks the workspace lockfile, including optional native
TLS dependencies. Vulnerabilities fail cargo-deny's advisory check; [deny.toml](deny.toml)
also denies yanked versions. The aggregate `ci` check requires this job to pass.

There are no ignored advisories. Any exception must identify a RustSec advisory
and include a written justification in an `{ id = "...", reason = "..." } entry.
Bare IDs and crate/version ignores are forbidden. CI and the local source replay
enforce that format with `python3 tools/verify-advisory-policy.py --self-test`.
The advisory check needs network access to current RustSec and registry data;
offline policy validation does not establish that dependencies are advisory-free.

## Numeric ownership and consumer features

Exact Decimal arithmetic belongs to `openbindings-internal-json/src/decimal.rs`.
It retains the normalized coefficient and signed decimal exponent implementation
and its tests; no power expansion occurs. Divisibility has the same finite
coefficient/exponent admission and explicit arithmetic-limit refusal. Public
semantic equality, preference validation, adapter integer classification and
uniqueness share this implementation.

`numeric.rs` registers exact comparisons and division with documented
`with_keyword` / `Keyword` APIs. The evaluator's `operands.rs` replaces numeric
bounds with positive handles and const/enum with handles into exact flat storage.
Every size operand, including minContains/maxContains, is normalized to a
nonnegative u64, saturating only above the physically realizable collection
size. Native contains and its annotations remain the library's responsibility.
Type and uniqueItems use documented JsonNumber::is_integer and Array::is_unique
overrides; fixed schemas use exact comparisons with their trusted small operands.
Original schemas are validated before substitution.

Inspection resources substitute numeric leaves without changing their shape.
Their pointer-identity table is built alongside each stable boxed copy, paired
structurally with original occurrences, never inferred from JSON equality.
Runtime diagnostic schema facts use the projection's original-source provenance.

The private FlatJson adapter redacts cold Node::to_value/JsonNumber::to_number
payloads. These documented cold APIs cannot represent every exact JSON number.
They are never the source of a verdict or a reported instance/schema fact:
equality, uniqueness, integer classification and numeric keywords have exact
implementations. The SDK does not expose library error Display/Debug or payloads.
Explanation kind/path metadata still comes from the library's validate(); bounds
and enum choices are read from original schema tokens. The cheap check never
requests library errors. An explanation retains the library's documented
allocation/stack exposure; this is not a new bounded diagnostic implementation.

Shared serde_json enables raw_value and unbounded_depth for exact RawValue
interoperation and admitted schema decoding. Upstream jsonschema/value additionally
enable float_roundtrip. The SDK never enables arbitrary_precision. Consumers may
explicitly enable it on the same identity; Number protocol admission remains
checked and preserves only the spelling/value the consumer actually emits.
Ordinary Serde flatten/untagged behavior is covered by the external consumer tests.

Shared runtime version floors remain Serde 1.0.229, serde_json 1.0.151,
itoa 1.0.18, fluent-uri 0.4.1, url 2.5.8, reqwest 0.13.5, Tokio 1.53.2 and
bytes 1.12.1. The relocated arithmetic uses num-bigint 0.4.8 and num-traits
0.2.19; both floors are included in consumer resolution checks. SDK packages, retained forks, jsonschema-value and wasm-bindgen
remain exact at their declared versions. The workspace lock fixes qualification.
`tools/verify-consumer-compat.py` checks twelve configurations: baseline/AP,
core/AP, evaluator/AP, portable discovery, all three interfaces, and native discovery/AP.
It asserts one serde identity using actual normal/build feature trees and records
package counts. Run its `--resolution minimum` and `--resolution latest` modes for
dependency updates, retaining `--output` receipts. Offline latest means newest
cached resolvable graph, not a crates.io-head claim.

## jsonschema

Upstream revision: `55ac1664384793c5d3b20745d3f9c169dfe2c06f`.
Applied package: `openbindings-internal-jsonschema 0.58.6-ob.1`.
The complete remaining Rust delta is **+305/-19 lines in eleven files** against the
0.58.6 archive (physical added/removed lines, including comments and blanks):

| File under src/ | Added | Removed | Why retained |
| --- | ---: | ---: | --- |
| canonical/context.rs | 2 | 2 | Use the ECMA matcher without Rust-regex translation in canonical pattern compilation. |
| compiler.rs | 3 | 3 | Cache and compile that same matcher, preserving authored pattern syntax. |
| keywords/additional_properties.rs | 2 | 2 | Route both specialized patternProperties/additionalProperties paths through ECMA matching. |
| keywords/unevaluated_properties.rs | 1 | 1 | Keep annotation matching on the same ECMA engine. |
| lib.rs | 6 | 1 | Export private adapter work and regex controls; align explanatory vocabulary. |
| node.rs | 38 | 1 | Charge schema entries/depth and signal detected cycles; the outer adapter refuses any stopped result. |
| ob_ecma.rs | 117 | 0 | Select regress ES2020 Unicode behavior, scoped regex work, and the qualified Unicode-property refusal. |
| ob_work.rs | 127 | 0 | Scoped work/depth/cycle/arithmetic stop accounting, including nested scope restoration. |
| properties.rs | 1 | 1 | Use the same matcher type for property validators. |
| regex.rs | 4 | 4 | Route the library's internal matcher/builder through regress. |
| validator.rs | 4 | 4 | Rename test-local identifiers while preserving upstream fixture values. |

These guards have no complete equivalent in documented upstream APIs. All
ordinary resolution, compilation, applicators and error construction remain the
library's implementation. Numeric semantics use its public extension APIs instead
of forked equality/type/size internals. Cargo changes only select the private
package/regress identities and ordinary upstream value/registry/serde dependencies.

The entire diagnostic patch is removed: admission/count/metadata/copy/payload and
schema-detail scopes, the property-name copy hook, diagnostic_text, lazy custom
error alterations, error-vector guards and validator-list coordinate alteration.
Fixed checks use is_valid and build_map/get/is_valid; value diagnostics are Coarse
by default and validate() first-error only on explicit Upstream request.
`tools/verify-diagnostics.py` continues to enforce the pinned allocation ceilings
and value allocation checks. No construction-memory guarantee is made for Upstream.

## regress

The upstream 0.12.0 archive contains the reversed `prohibit-unsafe` branch in
`unicode.rs::uppercase`; it was not introduced by the SDK patch. Both case-table
lookups now use checked indexing. `position.rs` uses checked non-null construction,
provenance-preserving wrapping pointer arithmetic and byte-address subtraction.
These produce the same positions/distances for valid input ranges without unsafe
blocks, including empty inputs and one-past-end positions. Both modules forbid
unsafe code at compile time. Tests cover every Unicode table entry and gap,
case-insensitive matching, and position arithmetic. Run those regressions with
`cargo test --manifest-path vendor/openbindings-internal-regress/Cargo.toml --no-default-features --features std,utf16,prohibit-unsafe --lib`.

- `Cargo.toml`, `src/lib.rs`: private identity and internal modules; the SDK explicitly selects the classical interpreter required by the work budget.
- `src/api.rs`, `src/classicalbacktrack.rs`, `src/ob_budget.rs`: finite interpreter work and cancellation propagation; a failed/exhausted match must never become an ordinary non-match that supports a wrong validation verdict.
- `src/parse.rs`, `src/ob_ecma11_properties.rs`: ECMAScript 11 property-name recognition and bounded pattern expansion/admission. Supported syntax and selected matching capability are distinct. Unicode property matching remains an explicitly qualified refusal only when selected by evaluation; irrelevant branches retain existing verdicts.
- Empty quantifier semantics: `src/classicalbacktrack.rs` treats an unrepresentable single-character atom as matching zero characters only when its quantifier admits zero repetitions, including lazy extension. Lone-surrogate escapes still match no scalar code point. `tests/fixtures/lone-surrogate.mjs` computes the checked-in 470-row differential fixture using Node's `RegExp` with the `u` flag (Node v22.19.0, V8 12.4.254.21-node.29); `tests/tests.rs` asserts every row, and the evaluator's `pattern_correctness` integration test runs every row through `pattern` and `patternProperties`.
- Backtrack budget restoration: both mid-interpreter budget exits truncate the backtrack stack to its exhaustion sentinel. The direct matcher regression uses `^(a+)+$` on 28 `a` characters plus `b`, without `catch_unwind`, at budgets 0–255 in debug tests; positive/negative lookaround variants and fresh-scope recovery are also checked. Re-run the fork with `cargo test --manifest-path vendor/openbindings-internal-regress/Cargo.toml --no-default-features --features std` to select the SDK's classical matcher configuration.

`OPENBINDINGS-PATCH.md` identifies each retained fork and links this contract.

## Invariants and regression gates

Keep exact input in the SDK's flat immutable arena. Do not convert values through f64 or serde_json::Value as an authoritative convenience path. The private compiler projection can strip annotations only because the declared evaluator is a validity evaluator with format assertion disabled; original SDK documents and diagnostics preserve original content. The adapter's const/enum and numeric keywords preserve exact values and charge work.

Core resources are explicit. The evaluator denies retrieval; static inspection uses a recording retriever with inert resources that never supply a verdict. A host enabling dependency HTTP/file features must still fail to produce any HTTP/FIFO trap hit through the SDK; direct dependency positive controls prove the traps work.

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

For an isolated rehearsal of the current `jsonschema` fork, use a new output
directory and the original crate archive directory:

```sh
python3 tools/rehearse-dependency.py --package jsonschema \
  --upstream-dir /path/to/original-crate-archives \
  --output target/dependency-rehearsal
```

This checks the upstream checksum, extracts pristine sources, applies the
maintained patch with the standard `patch` executable, and compares every
reconstructed file to the current fork. Keep `RECONSTRUCTION.json` and the patch
log. Independently regenerate the patch inventory with `dependency-patches.py`
into another fresh directory and compare it with `docs/dependency-patches`.
Then run the applicable source, consumer compatibility, evaluator and packaged
artifact gates described above on the same committed candidate. Exact source
reconstruction is a maintenance rehearsal; it neither upgrades the dependency
nor establishes that a future upstream rebase will be straightforward.

The maintained SDK exact-operand, ECMA, resolver, diagnostic, corpus and kit tests
govern this supported graph. Re-run upstream source suites when updating the
corresponding library and investigate each changed result explicitly. Historical
upstream-test dispositions are archived outside the product and are not skip filters.
