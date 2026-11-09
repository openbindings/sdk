# Maintaining the SDK

This page holds build, resolver and qualification details. Application guidance
starts in the [SDK README](../README.md).

## Build and verify

Development uses Rust 1.99.0 with `wasm32-unknown-unknown`, wasm-bindgen-cli 0.2.129,
Node 22.23.0/npm 10.9.8 (the first Node 22 release whose bundled undici rejects a
chunked body cut short as its connection closes, [nodejs/undici#5273](https://github.com/nodejs/undici/pull/5273)) and
Python 3.13+. Rust 1.88 is the declared MSRV. Use
tracked dependency locks. Offline builds require the tools and dependencies to be
cached already. [Source replay](REPLAY.md) describes acquisition and host setup.

```sh
cargo test --locked --workspace --features openbindings-http-discovery/native
cargo test --locked --workspace --doc --features openbindings-http-discovery/native
python3 tools/verify.py
```

Build the facade with `npm run build:wasm` and `npm run build` in
`packages/typescript`. Run `npm test`, `npm run test:types` and `npm run test:package`
there. The package check installs a real archive and runs its README and packaged
examples. `python3 tools/verify.py --browser` adds browser checks.

`tools/verify-api-reference.py` checks Rust documentation and TypeScript reference
contracts. `tools/verify-rust-snippets.py` executes prose examples, including
the external document-to-typed-result walkthrough.
`tools/verify-kind-adapter.py` checks the adapter guide's verbatim Rust blocks
against its external example, then tests and lints that example with its lockfile.
Add `--wasm` to build its library for `wasm32-unknown-unknown`.
`tools/verify-product-labels.py --self-test` checks product vocabulary boundaries.
`tools/verify-doc-links.py` checks local Markdown targets and fragments without
network access, including repository URLs that name this checkout. Remote URLs
are reported as unchecked. `tools/verify-fixture-digests.py --self-test` checks
that each vendored interface fixture matches the SHA-256 row in its
`tests/fixtures/README.md`; update a row only with the snapshot it describes. See [release preparation](release-candidate.md) for
archive and source-identity checks and [API reference](api-reference.md) for rendering.

On pull requests and pushes to `main`, the [CI workflow](../.github/workflows/ci.yml)
also runs the workspace advisory check with all features and the locked graph.
Vulnerabilities and yanked versions fail the `advisories` job, which is required
by the aggregate `ci` check alongside the `native`, `package`, `browser`,
`node-floors`, `wasm-reproducibility` and `msrv` jobs.
See [advisory policy](../DEPENDENCY-MAINTENANCE.md#advisory-policy) for the local
command and the required advisory ID and reason for every exception. This check
needs network access; `tools/verify-advisory-policy.py --self-test` checks only
the repository policy offline.

The `package` job builds the Wasm and TypeScript package, runs its archive and
Wasm qualification and the TypeScript tests on Node 22.23.0, and hands
`packages/typescript/dist` to the `browser` and `node-floors` jobs. The
`node-floors` job runs the TypeScript tests on that build at the other
supported lines' floors, Node 24.17.0 and 26.2.0.
The `browser` job runs the Chromium and WebKit replay and judges inside Microsoft's
Playwright image, which already contains the browsers and their system packages,
so CI installs no browser dependencies. The image is pinned by its
multi-architecture index digest at the `playwright-core` version in
`packages/typescript/package-lock.json`. The job's first check fails when the
Playwright release the image records, or the tag the workflow declares, differs
from the lock. When the lock changes `playwright-core`, update the tag and digest
together; `docker buildx imagetools inspect mcr.microsoft.com/playwright:v<version>-noble`
prints the index digest.

Hash tables must never use input-chosen content as keys, including content hashes
and tuples containing text or bytes. Use ordered maps, ordered sets or sorted
sequences so worst-case work needs no secret seed or host entropy; SDK-assigned
positions and content-free identities may still use hash tables. Fingerprints
may select sorted candidates only when exact comparisons are charged.

## Resolver implementation

The bundled resolver and default evaluator give a unique contained declared ID
priority over supplied names; packaged standards are the final fallback. Supplied
retrieval aliases and declared IDs compete equally when no contained ID owns the
name. A reached carrier must itself own its canonical ID: aliases, pointers and
direct applicators into a competing carrier conservatively refuse preparation.
Unused conflicts do not poison other schemas, and an independently named nested
resource retains its original base and dialect. No schema-body comparison, alias
transfer or missing-fragment fallback occurs. `EvaluationRequest::external_schemas`
borrows the original catalog. `SchemaOrigin::source_uri` records original retrieval
provenance, which need not be the winning lookup association for that URI. Replacing
resources requires a new immutable set and preparation; retained schemas keep their inputs.
OBI-13 retains exact identifier comparison; ordinary JSON Schema resolution uses
the original resource inventory and compact identifier comparisons.


Core retains OpenBindings embedding rules and original reference meaning. The
compiled evaluator lowers established references into a private bundle; Original
uses the retained schema environment directly. See
[architecture](architecture.md) and the
[dependency maintenance contract](../DEPENDENCY-MAINTENANCE.md) for ownership,
projection and upgrade checks.

The separate performance workflow enforces outcome and allocation checks;
timing is advisory and it is not part of the required `ci` aggregate. Local
workerd measurements likewise do not constitute a required hosted workerd job.
Candidate performance and deployment qualification need recorded runs of their
own; see [performance boundaries](performance-boundaries.md).

## Findings and allocation qualification

Fixed document checks use validity decisions separately from location guidance.
The location walk follows failing members and elements and retains independently
failing containers. A findings-allowance or frame-depth stop preserves an
established violation and marks findings truncated. OBI-10 locates each keyword
of a failing schema whose value fails that keyword's own meta-schema check, or
the schema itself when it is neither an object nor a boolean. The localized
meta-schemas constrain a schema only through its type and its keywords' values,
so those per-keyword checks, built once at the first failure, locate exactly
the keywords that make it fail. No optional value
error tree participates in document rule evidence.

[Allocation qualification](../tools/diagnostic-memory-probe/README.md) records
measurement boundaries and fixture-specific thresholds. The
[performance page](performance-boundaries.md) gives reproducible caller-facing
measurements. [Stack-test scope](../tools/stack-margin-tests.json) records the
MSRV exclusion without weakening development-toolchain tests.

## Dependency and fixture maintenance

Use [dependency maintenance](../DEPENDENCY-MAINTENANCE.md) for upstream source
records, the evaluator's schema transforms and upgrade invariants. The
[evaluator fixture provenance](../crates/openbindings-schema-evaluator-test-support/fixtures/provenance.json)
records frozen inputs and policy translations. The
[qualification guide](../tools/qualification/README.md) describes corpus replay.
Fixture counts and proof bookkeeping belong to these records rather than package
introductions. Regenerate attribution with `tools/attribution.py` when dependencies
change; preserve the distinction between supported public crates and internal
delivery dependencies in `tools/package-candidate.py`.

## Maintained verification

After generating the current package (`npm run build:wasm` then `npm run build` in
`packages/typescript`), run from the repository root:

```sh
python3 tools/verify-api-reference.py target/api-reference
node --test packages/typescript/test/api-reference.test.mjs
```

The standalone verifier denies missing Rust reference documentation and broken
intra-doc links, compiles Rustdoc examples, checks actual emitted TypeScript exports
and public/inherited members, and strictly compiles the reference consumer.
Component CI requires the Rust checks through
`tools/verify.py` and the TypeScript checks after building the package. Pass
`--rust-only` or `--typescript-only` to run the corresponding half.
A docs-only developer check may emit declarations with `tsc -p
packages/typescript/tsconfig.json` using existing generated binding declarations;
that checks types and documentation, not a source-bound Wasm artifact.

The negative controls remove documentation from temporary emitted copies of a
reexported type, a coordinate field and an inherited disposal method. Each must be
rejected. Definition presence is an omission gate, **not a quality percentage**.
Review rendered pages and compiled examples for truthful stage distinctions,
actionable errors, ownership, defaults/units, capability limits and retention. The
release checklist records those SDK-specific outcomes separately from source landing.



## Invocation qualification

Maintained request-value and lifecycle tests cover the adopted interface and
this SDK's cancellation, half-close, output ordering and terminal-event policy.
The archived frame-mapper experiment does not establish a streaming fixture
standard. The external Catalog consumer explicitly pulls, adopts additions, then
invokes a selected binding or operation from its conformant document. The
invoker-only consumer guards dependency isolation; the Node/Wasm probe qualifies
a thin Rust bridge, not a released TypeScript API or browser deployment. Run
`tools/verify-invocation.py` for the maintained consumer qualification.

Context provisioning and protocol transports remain application responsibilities.
The SDK supplies no kind-specific instances. The separate
[OpenAPI project](https://github.com/openbindings/openapi) implements the interfaces.
Current operation consumer checks cover aliases, selection, rejected values,
context challenges and explicit retries without implicit effects.
The resource probe records warmed binding-versus-operation overhead and owner
release; it does not claim HTTP parity, browser memory bounds, or arbitrary schema
throughput.

## Generated lane comparisons and fuzzing

Run the seeded default-limit differential with
`cargo test -p openbindings-json-schema-evaluator --lib default_limit_lane_differential -- --nocapture`.
It generates 1,008 schemas, probes their boundaries, and prints verdict and
compiled-route counts. The test asserts a compiled comparison floor. For a larger
reproducible run, prefix the command with `OB_DIFFERENTIAL_CASES=20000` and
`OB_DIFFERENTIAL_SEEDS=2f6b95d1c0a47e33,1,deadbeef,9e3779b97f4a7c15`.
The differential compares complete outcomes against independently prepared Original
and retains seed and case coordinates for reproducing a disagreement.
The older work-capped reference-placement test remains separate.

`tools/verify.py` also runs the [fuzz harness](../fuzz/README.md)'s committed seed
corpus on stable: `cargo test --locked --manifest-path fuzz/seeds/Cargo.toml --test seed_corpus`.
For coverage-guided runs, install `nightly-2026-11-01`, and `cargo-fuzz` 0.13.2 with the
repository's stable toolchain (its locked dependencies do not build on that nightly), then
run `cargo +nightly-2026-11-01 fuzz run lanes -- -max_total_time=120 -max_len=16384`
from the repository root. Substitute `document`, `json_value`, `decimal` or
`pattern` for the other targets. Crashes go to `fuzz/artifacts/<target>/`; the
weekly/manual `Fuzz` workflow uploads them as `fuzz-artifacts-<run-id>`.
See the harness guide for replay, minimization and campaign cost.
