# Internal dependency maintenance contract

Core uses the unmodified jsonschema 0.58.6 library, with its default pattern
engine (jsonschema-regex 0.58.6 and fancy-regex 0.19.2), for its fixed checks
(OBI-02 and OBI-10) and for the resource identity and reference resolution of
its reference inspection. The SDK still carries two renamed forks, jsonschema
0.58.6 and regress 0.12.0, for the default evaluator alone, until that evaluator
moves to the unmodified library too. Both are private implementation
dependencies. jsonschema-value 0.58.6, referencing 0.58.6 and serde_json 1.0.151
use their unmodified registry identities and serve both. `docs/dependency-isolation.json`
records the six upstream archive checksums and complete Rust source digests.
`tools/verify-dependency-isolation.py` checks those sources, the single-serde
graph, absence of SDK-requested arbitrary precision, retained fork identities,
that core's own graph holds neither fork, the separation below, and the
cancellation/schema boundaries. Its optional `--upstream-dir` additionally checks
the downloaded archives.

## Core on the unmodified library

Core's fixed checks compile the pinned document schema and the localized
2020-12 meta-schemas with `jsonschema::options_for::<FlatJson>()`, exact
comparisons registered through `with_keyword`, format assertion off, only local
resources, and the library's default `PatternOptions`. They have no work budget.
Every fixed pattern is regular: the SemVer `openbindings` pattern, the name
grammar, `^x-`, `^[^#]*#?$` for `$id` and the anchor grammar. jsonschema-regex
leaves each unchanged (`^x-` takes its literal prefix path), and fancy-regex hands
a pattern without lookaround or backreference whole to the regex crate's
finite-automaton engine, whose time is linear in the string and whose memory is
bounded by the pattern. The derived document schema has no recursive reference
and the meta-schema check cuts its recursive edges, so a node meets each schema
position at most once (80 and 129 positions; tests count them), and location
guidance revalidates only beneath failing positions, a bounded number of times
for each level of the derived schema's finite depth. The admission limits
therefore bound each check's work; its memory is the stored document, a bounded
matcher cache and the capped findings; and its stack depth follows the fixed
schemas, not the instance (`document_stack` runs 12,000-deep schemas on a
256 KiB thread). The library catches the regex crate's known search panic
(rust-lang/regex#1344) only under `panic = "unwind"`; no fixed pattern has
reached it in the differential's matches.

Three regressions guard these facts:

- `fixed_patterns_match_as_ecmascript_does_through_every_keyword` runs each
  pinned pattern through `pattern`, `propertyNames`, `patternProperties` and the
  fused `patternProperties`/`additionalProperties` path, under the fixed-string
  presentation of unpaired surrogates, over a 60,000-string generated corpus,
  and compares every result with V8's `new RegExp(pattern, "u").test(string)`
  recorded in `crates/openbindings-internal-schema/tests/fixtures/fixed-pattern-ecma.json`.
  Its generator, `fixed-pattern-ecma.mjs`, regenerates the corpus from a seed;
  the TypeScript suite derives the recorded results from V8 again on every run.
  The test also requires the fixture's patterns to be exactly the pinned schemas'.
- `fixed_checks_run_their_patterns_on_the_default_engines_finite_automaton`
  pins the engine choice: lookaround compiles, so the default is still
  fancy-regex, and a valid version that would exceed fancy-regex's default limit
  of a million backtracking steps on its backtracking path still matches.
- The memory probe's near-miss phase (`tools/verify-diagnostics.py`) requires
  fixed-check peak bytes and allocation counts not to grow from 1 MiB to 8 MiB
  near misses of the SemVer, name, anchor and identifier patterns.

The source digests of jsonschema, jsonschema-regex and fancy-regex are recorded,
so an upgrade of any of them fails the isolation check until it is reviewed: run
the three regressions above, the 440-case specification corpus, and the document
validation differential against the previous release, then refresh the records.

### Separation from the evaluator's fork

The full workspace graph holds both `jsonschema` (registry) and
`openbindings-internal-jsonschema` (the fork), which share the registry
jsonschema-value, jsonschema-regex, fancy-regex, regex and referencing packages;
only the library itself is compiled twice. That costs about 1.3 MB of the
TypeScript package's Wasm module (250 KB compressed), and a process that
prepares a value contract compiles each library's own 2020-12 meta-schema
validator once (about 0.5 MB, held for the process). Both costs end when the
evaluator moves to the unmodified library. Core's graph, built alone or by a
consumer of core alone, holds neither fork. The two never exchange a value:

- `openbindings-internal-schema` names only the registry library and
  `openbindings` names neither; the evaluator names only the fork, under the
  dependency name `jsonschema`.
- `openbindings-internal-schema` and `openbindings-internal-json` expose no
  library type. The evaluator reaches core's schema space, with its projections,
  reference resolution and the meta-schema check of every reached schema,
  through SDK types only, and builds its own registry and validators with the
  fork. `FlatJson` implements the shared jsonschema-value traits once for both.
- `openbindings-internal-json/src/numeric.rs` holds only exact arithmetic; each
  library's adapter registers it: core's fixed checks the four comparisons, the
  evaluator all five keywords (`operands.rs`).
- `FlatJson` charges its reads to the fork's evaluation work scope only under
  `openbindings-internal-json`'s `evaluator-work` feature, which only the
  evaluator enables. Without it, as in core's own graph, a charge is free.
  Within a workspace build the feature is on, but core opens no fork scope, so
  its charges stop nothing.

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

`numeric.rs` holds exact comparisons and division; each library's adapter
registers them with its documented `with_keyword` / `Keyword` APIs. The
evaluator's `operands.rs` replaces numeric bounds with positive handles and
const/enum with handles into exact flat storage.
Every size operand, including minContains/maxContains, is normalized to a
nonnegative u64, saturating only above the physically realizable collection
size. Native contains and its annotations remain the library's responsibility.
Type and uniqueItems use documented JsonNumber::is_integer and Array::is_unique
overrides; fixed schemas use exact comparisons with their trusted small integer
operands, and assert no `multipleOf` (a test checks every position).
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
Applied package: `openbindings-internal-jsonschema 0.58.6-ob.1`, used by the
default evaluator alone.
The complete remaining Rust delta is **+581/-32 lines in fifteen files** against the
0.58.6 archive (physical added/removed lines, including comments and blanks):

| File under src/ | Added | Removed | Why retained |
| --- | ---: | ---: | --- |
| canonical/context.rs | 2 | 2 | Use the ECMA matcher without Rust-regex translation in canonical pattern compilation. |
| compiler.rs | 96 | 18 | Cache and compile that same matcher, preserving authored pattern syntax. Charge each compiled node, with its location and dynamic scope, and each deferred target's placeholder copy to the compilation scope; meta-validation stays uncharged. In a build that reaches no dynamic anchor, compare and hash compiled-node cache keys without the dynamic scope, so a schema reached along many resource paths compiles once. Release a build that stops part way as a finished validator is released, its deferred targets last and in order, so a long `$ref` chain never drops recursively. |
| keywords/additional_properties.rs | 2 | 2 | Route both specialized patternProperties/additionalProperties paths through ECMA matching. |
| keywords/dependencies.rs | 2 | 0 | Charge each `dependentRequired` entry's node and name. |
| keywords/pattern.rs | 2 | 0 | Charge the pattern bytes each compilation analyzes and copies. |
| keywords/ref_.rs | 39 | 0 | Charge each reference resolved and each dynamic-scope entry searched for a dynamic anchor. Fail a static build when a reference resolves through a dynamic or recursive anchor, rather than compile it with caches that ignore the scope; the unevaluated walks share the check. |
| keywords/required.rs | 16 | 0 | Charge the property names each `required` list copies. |
| keywords/unevaluated_items.rs | 6 | 0 | Apply the static-build check to each reference the walk resolves itself, including `$dynamicRef` and `$recursiveRef` in drafts that do not compile them. |
| keywords/unevaluated_properties.rs | 11 | 4 | Keep annotation matching on the same ECMA engine; build only the walk's `patternProperties` patterns (see compiled subschema ownership); apply the static-build check to each reference the walk resolves. |
| lib.rs | 5 | 0 | Export private adapter work and regex controls. |
| node.rs | 38 | 1 | Charge schema entries/depth and signal detected cycles; the outer adapter refuses any stopped result. |
| ob_ecma.rs | 117 | 0 | Select regress ES2020 Unicode behavior, scoped regex work, and the qualified Unicode-property refusal. |
| ob_work.rs | 240 | 0 | Scoped work/depth/cycle/arithmetic stop accounting, including nested scope restoration, and a separate compilation work scope that bounds a schema build, weighs each kind of compilation work, and carries the caller's statement that the build reaches no dynamic anchor. |
| properties.rs | 1 | 1 | Use the same matcher type for property validators. |
| regex.rs | 4 | 4 | Route the library's internal matcher/builder through regress. |

These guards have no complete equivalent in documented upstream APIs. Apart from
the ownership correction below, all ordinary resolution, compilation, applicators
and error construction remain the library's implementation. Upstream code, tests and comments keep their own wording:
the SDK's vocabulary governs its own sources, not retained upstream text. Numeric semantics use its public extension APIs instead
of forked equality/type/size internals. Cargo changes only select the private
package/regress identities and ordinary upstream value/registry/serde dependencies.

The entire diagnostic patch is removed: admission/count/metadata/copy/payload and
schema-detail scopes, the property-name copy hook, diagnostic_text, lazy custom
error alterations, error-vector guards and validator-list coordinate alteration.
Core's fixed checks, on the unmodified library, use is_valid and
build_map/get/is_valid; the evaluator's value checks use is_valid, and validate()
runs for its first error only when a caller explains a failing value. `tools/verify-diagnostics.py` continues to enforce the pinned allocation
ceilings and value allocation checks. No construction-memory guarantee is made for
an explanation's library diagnostics.

### Compiled subschema ownership

Upstream's `unevaluatedProperties` walk compiles each `patternProperties` subschema
it reaches, then drops the node and keeps only the pattern. The walk compiles at
its referrer's location, so that node, and any `$ref` target first compiled
beneath it, has no owner but the build caches. The compiler links a later
compilation round to an unsettled node of an earlier round by a non-owning edge,
on the premise that the earlier round's tree owns that node. After the build such
a target is freed, and validation panics with "pending schema target dropped"; a
Wasm host aborts. Unmodified jsonschema 0.58.6 panics on a seven-resource program
the SDK projects. Release 0.59.0 and upstream master still compile and drop.

The fork's walk builds only the patterns. Marking needs nothing else: a 2020-12
`patternProperties` annotation is the set of matched names, and nothing read the
dropped node. The subschemas stay compiled, and owned, where `patternProperties`
validates them. Every schema the walk visits is also compiled as a node, by its
enclosing schema, by a `$ref` or `$dynamicRef` keyword (possibly in a later
round), or as one of the walk's own `allOf`/`anyOf`/`oneOf`/`if` nodes; that node's
`patternProperties` keyword, or the fused `additionalProperties` validator,
compiles the same subschemas and raises the same errors. The walk's copy raised
an error first only for a resource root reached through `$ref: ""`, which
validation skips, or under a custom keyword replacing `patternProperties`; the SDK
uses neither. Its cache entries only decided which compilation came first: the
review corpora, step budgets and cancellation points are byte-identical to the
unmodified walk. Prepared contracts hold exactly the heap they held before, and
preparation requests less: 30 referrers reaching five recursive patterns request
1,470,560 instead of 1,671,146 bytes in 10,776 instead of 13,177 allocations, and
hold 178,183 bytes either way. Keeping the walk's unsettled nodes instead held
about 940 bytes and 13 allocations more per pattern and referrer. The benchmark's
`unevaluated-patterns` workload guards that retained heap.

Three compile sites still drop a node, none with this hazard. The walk's boolean
`allOf`/`anyOf`/`oneOf` branches are settled leaves, and a dropped settled node is
safe: every held non-owning edge (a shared reuse, a deferred target's placeholder,
a cycle's alias placeholder) points at an unsettled node, and reaching one makes
every enclosing node unsettled. The regex fallback after a literal
`patternProperties` attempt recompiles the same locations in the same round and
owns the cached nodes. `additionalProperties: true` beside `patternProperties`
compiles the pattern subschemas and returns no validator; the `patternProperties`
keyword compiles the same locations in the same round and owns those nodes,
unless a custom keyword replaces it, which the SDK never registers.

The evaluator's `unevaluated_pattern_ownership` test (and its Wasm counterpart)
runs the reduced reproducer in four placements; `reference_placement` compares
500 seeded random contracts, split across document, contained and supplied
resources, with the same schemas in one resource. Both fail on the unmodified walk.

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

Core resources are explicit. The evaluator denies retrieval; static inspection uses a recording retriever with inert resources that never supply a verdict, and the fixed checks retrieve only their own local resources. A host enabling dependency HTTP/file features on both schema libraries must still fail to produce any HTTP/FIFO trap hit through the SDK; direct positive controls on each library prove the traps work.

Before adopting an upstream update:

1. Archive/checksum the new upstream sources and record its Git revision. Compare every old patch against the new API and behavior; never copy a Cargo registry cache over the production fork.
2. Prefer an equivalent upstream hook and remove a patch when original-value, scope, work and diagnostic invariants remain provable. Keep unique private names until the unmodified upstream can satisfy them.
3. Rebase the change in isolation. Re-run the full pinned 440-case OpenBindings corpus and the evaluator kit's 1,564 observations, with its exact-case refusal declarations, natively and in real Wasm (`python3 tools/verify.py --browser`). Run the affected upstream library tests, negative controls, no-I/O feature-unification traps, dynamic-scope/isolation, cancellation, deep literals, exact arithmetic and ownership probes.
4. Repeat the frozen representative performance/memory series (`tools/evaluator-bench`) and packed consumer tests. A verdict becoming a refusal is a regression unless the governing authority intentionally changes; update scope only through an explicit separate decision.
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
