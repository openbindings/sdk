# Internal dependency maintenance contract

The SDK carries five renamed internal dependency packages: three behaviorally modified forks and two mechanical package-identity forks. They are implementation details, not supported extension APIs. Their upstream versions and complete crate checksums are recorded by `tools/dependency-patches.py`; that script compares every file with the original downloaded crate archive and emits a full diff plus before/after digests. The original upstream Git identities are retained as `UPSTREAM-VCS.json`; original manifests are retained as `UPSTREAM-Cargo.toml`. Cargo-reserved generated filenames were renamed so the internal packages can themselves be archived. Cargo extraction completion stamps are removed. Retained upstream lockfiles are provenance; the SDK workspace lock is the development/build authority.

| Upstream | Applied package | Upstream revision |
| --- | --- | --- |
| jsonschema 0.58.6 | openbindings-internal-jsonschema 0.58.6-ob.1 | 55ac1664384793c5d3b20745d3f9c169dfe2c06f |
| jsonschema-value 0.58.6 | openbindings-internal-jsonschema-value 0.58.6-ob.1 | 55ac1664384793c5d3b20745d3f9c169dfe2c06f |
| regress 0.12.0 | openbindings-internal-regress 0.12.0-ob.1 | See its retained upstream VCS record and archive checksum |
| serde_json 1.0.151 | openbindings-internal-serde-json 1.0.151-ob.1 | See its retained upstream VCS record and archive checksum; Rust source unchanged |
| referencing 0.58.6 | openbindings-internal-referencing 0.58.6-ob.1 | 55ac1664384793c5d3b20745d3f9c169dfe2c06f; Rust source unchanged |

## Consumer dependency policy and isolation

Adding the SDK must preserve ordinary `serde_json` numeric buffering behavior.
Both mandatory validator packages require arbitrary precision. A Cargo alias or
optional ordinary-value conversion cannot isolate that feature. The SDK therefore
uses a distinct internal `serde_json` package throughout its validator graph.
`referencing` must use that same identity because its registry, resources and
retriever interfaces carry `serde_json::Value` into the compiler. Its own distinct
package prevents an application's unrelated `referencing` dependency acquiring
the private value identity. A shared `serde_json` feature bridge enables only
`raw_value`, preserving the existing public `JsonValue` Serde protocol even when
the consumer declares plain `serde_json`. It enables neither arbitrary precision,
float roundtripping nor unbounded depth. Rust implementation code uses the private
copy; the shared edge exists only to retain protocol recognition in consumers.

The alternative of removing `Number` from the existing exact adapter requires
changes to numeric schema projection, cold `Value`/`LazyInstance` materialization,
`JsonNumber` interfaces and validator numeric/canonical paths. These paths admit
values such as `1e400` that ordinary `serde_json::Number` cannot represent. A
floating-point or placeholder substitution would lose the existing contract.
The selected identity changes add upstream-update bookkeeping while preserving
Rust source byte-for-byte in both added packages. They avoid adding numerical
behavior to rebase. They can add a second JSON implementation to a native consumer
that also uses upstream `serde_json`; source bytes do not establish executable
size or startup cost. Only the internal JSON package is referenced by SDK Rust implementation code; the raw-value feature bridge is also present in the graph. Linked Wasm cost must be measured.

Public SDK and evaluator APIs expose SDK values and shared Serde traits; the
private JSON and registry types are not supported public extension types. The
ordinary-value conversion still checks the emitted `$serde_json::private::Number`
protocol as exactly one valid JSON number, with its existing budgets and error
locations. Consumers explicitly enabling upstream arbitrary precision retain
that upstream behavior, including its decimal flatten/untagged limitations. The
SDK preserves emitted Number tokens and cannot restore digits already rounded
by the consumer. Exact parsing and RawValue serialization retain source spelling.

Shared direct runtime requirements use caret ranges from the tested floors:
Serde 1.0.229, serde_json 1.0.151 (raw-value protocol only), itoa 1.0.18,
fluent-uri 0.4.1, and, for native HTTP, reqwest 0.13.5,
Tokio 1.53.2 and bytes 1.12.1. These are supported direct dependency floors, not
a claim about the minimum version of every transitive dependency. Coupled SDK
packages, internal forks and the Wasm binding generator remain exact. The
workspace lock fixes CI. Contradictory exact consumer requirements need not
resolve. The external regression witness currently qualifies upstream serde_json
1.0.151; arbitrary precision is confined to the distinct internal identity.

`tools/verify.py` runs the source-identity guard and seven external consumer
configurations: baseline, explicit arbitrary precision, core with and without
explicit arbitrary precision, evaluator, and native discovery with and without
explicit arbitrary precision. The hook checks actual Cargo build feature trees;
`cargo metadata` alone also reports weak optional macro feature edges and can
overstate the activated graph. Checked Number shape/injection/limit cases and
cross-package Number/RawValue cases are maintained in `tools/consumer-compat`.

For a dependency update, additionally run `tools/verify-consumer-compat.py
--resolution minimum` and `--resolution latest`, retaining `--output` receipts.
The first selects the documented direct floors; the latter asks Cargo for the
newest resolvable graph. Use `CARGO_NET_OFFLINE=true` only when intentionally
qualifying the cached graph and label that limitation. The initial isolation
qualification's minimum, locked and newest offline-resolvable shared versions
coincide; it does not claim multiple versions were tested or a crates.io head run.

## Mechanical serde_json and referencing updates

Their Rust files, including build scripts and upstream tests, must match the
downloaded upstream archives exactly. `docs/dependency-isolation.json` records
archive checksums and every Rust source digest; `tools/verify-dependency-isolation.py`
checks the entire file set and dependency wiring on every source replay. Pass
`--upstream-dir <crate-archives>` to also verify those archive identities and
their source digests. No Rust-source modification is approved by this policy.

When updating either package: acquire and checksum the upstream crate; retain its
licenses, original manifest and VCS record; rename package identity; apply only
the exact sibling dependency pins; regenerate the source manifest from the
archive, never from the modified checkout; compare the complete patch inventory;
then run source identity, all consumer modes, D03/API-quality and exact evaluator
tests, and applicable native/Wasm/package/browser qualification. Serde protocol
changes require explicit cross-package checks. Follow the broader update gates
below for behavioral dependency changes. Keep every `publish = false` guard.

## jsonschema

- `Cargo.toml`: unique private package identity and explicit sibling package dependencies prevent accidental resolution to an unpatched evaluator/value/regex implementation. Public crate names and upstream licenses are retained.
- `src/lib.rs`: exposes internal budget/regex support needed by the adapter.
- `src/ob_work.rs`, `src/node.rs`, `src/validator.rs`, `src/compiler.rs`: scoped evaluation steps, depth, recursion/cycle handling and diagnostic accumulation. Guards restore thread-local state after return, nested exhaustion and unwinding. Arithmetic exhaustion is separate from false validity. Entry and primitive traversal limits are not advertised as nanosecond deadlines.
- `src/ob_ecma.rs`, `src/regex.rs`, `src/canonical/context.rs`: route qualified dynamic patterns through the ECMAScript interpreter, preserve pattern preparation failures, and distinguish selected unsupported Unicode-property matching from work exhaustion.
- `src/error.rs`, `src/keywords/custom.rs`: retain lazy exact instance views through custom-keyword error context; avoid recursive materialization/drop of 10,000-level literals. Internal diagnostics expose bounded metadata and original locations, not whole instance values.
- `src/keywords/additional_properties.rs`, `any_of.rs`, `helpers.rs`, `items.rs`, `min_length.rs`, `properties.rs`, `property_names.rs`, `required.rs`, `unevaluated_items.rs`, `unevaluated_properties.rs`, and `src/properties.rs`: preserve generic flat-value operation, scoped evaluation/diagnostic work, ECMAScript matching and dynamic reference behavior in the affected applicator/keyword paths. Check the full diff when rebasing; these are evaluator-sensitive changes, not mechanical wrappers.

The value-diagnostic budget repair adds a private `diagnostic_metadata` scope to
`ob_work.rs`, with pre-copy path/member admission and deterministic usage counters.
`paths.rs` sizes escaped instance paths before any segment vector/string expansion
and omits unused evaluation trackers. Metadata mode skips unused applicator branch
contexts, source-controlled keyword payloads and owned rejected values before
construction; property-name temporaries are admitted explicitly. The ordinary
upstream diagnostic path and SDK document diagnostic count scope remain separate.
The adapter independently admits original source coordinates and final problems;
private copied bytes do not establish a total heap guarantee. Maintained native
`diagnostic_budget` and shared package/browser `diagnostic-budget-cases.mjs` cover
the witness, boundaries, pre-copy counters and deterministic seed 0x5eedcafe.

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

## Precise witness attribution

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
