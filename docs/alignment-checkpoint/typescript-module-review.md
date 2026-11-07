# Independent TypeScript module and external verifier review

Bounded **PASS**, 2026-11-03. Reviewed source matches checkpoint `da39d8b` (full commit and SHA-256 identities in `final-source-hashes.json`). No production source was changed. This covers the TypeScript organization and verifier extraction/isolation; it does not close the ordinary compiler allocation gate or qualify the subsequent evaluator changes.

## TypeScript behavior and boundaries

The AST/token comparison accounts for all 149 declarations formerly in `internal.ts` and HTTP discovery: 148 token-identical after removing export modifiers, plus `ownEntries` differing only by a trailing parameter comma. No declaration was added, removed, or duplicated. The facade preserves all 57 named exports. The graph is acyclic: diagnostics → ownership/runtime → exact JSON → schema → document → HTTP discovery; the main entry does not import HTTP. The diagram's arrow here means dependency progression, not import direction; exact adjacency is in `module-control-results.json`.

The single ownership module preserves the shared WeakMap/finalizer, canonical Wasm owner checks, idempotent disposal, and partial collection-adoption cleanup. HTTP publication uses the same conformant owner implementation; document initialization establishes `adoptDocument` before HTTP consumes it. The package export map stays unchanged, so implementation helpers remain absent from the public facade and seven attempted deep package imports are refused. Internal declaration stripping preserves the existing private helper boundary. Browser asset enumeration now includes every actual emitted module; stale `internal.js` is removed by the clean build and explicitly forbidden by the installed-package gate.

Independent controls on Node **22.23.0**:

- 35 existing initialization, exact ingress, canonical ownership, proof mutation, schema retention, API quality, and declaration-mutation controls pass (`selected-node-tests.log`). The earlier local Node 22.19 run was supplemental only; the retained gate log uses the supported pinned version.
- `public-entry-controls.mjs` imports HTTP and testing before the main entry, observes shared readiness and error identity, rejects an Example borrowed as an Operation, retains a prepared schema and publication after all originating owners are disposed, and returns live owners to the warmed baseline. Fixed meta-schema owners are warmed before that lifetime comparison.
- `module-controls.cjs` checks the declaration accounting, exact facade names, dependency cycles and helper exports. Package-name imports independently reject `internal`, `runtime`, `diagnostics`, `ownership`, `json`, `schema`, and `document`; the supported three entries work and share `SdkError`.

The production report's 180-test, installed-package, Chromium/WebKit, and workerd runs are additional parent evidence, not independently rerun here. The runtime lifetime fixture's `SchemaResources` → `ExternalSchemas` correction preserves its values, assertions, retained-contract lifecycle, and existing `PrepareOptions.resources` field. API-reference negative tests still mutate one unique declaration and still reject a leaked Wasm import; locating declarations by their unique name correctly accommodates the split.

## External verifier integrity

The shared `verify_resolution` body is AST-identical to the already reviewed invocation verifier after replacing global `ROOT` with its explicit argument. Both callers verify generated lock identities as a subset of the root lock plus named local fixture packages; metadata must be locked and each local package must resolve to its expected directory. The source-pull verifier additionally gains coherent pre/post input hashes and a retained generated-lock hash.

The build target is now below each role/mode's output (`invocation-qualification/{source,archives}/build` or `source-pull-qualification/{source,archives}/build`). Wasm-bindgen reads that same isolated target. Archive extraction and fixture copies refresh timestamps, avoiding the previously observed normalized-mtime artifact reuse. Source/archive and invocation/pull fixtures no longer compete for same-name build artifacts. The tools do not promise concurrent execution of the same role/mode against the same output directory.

Independent injected-command integration controls accept valid source/archive runs and reject main/isolated checkout leaks, changed lock identities, first-command failure and changed input snapshots. All five rejections leave no success receipt. Every injected consumer Cargo command observes the isolated target (`verifier-integration-results.json`). Eleven additional shared-helper controls cover valid lock subsets/fixtures, version/checksum/source changes, unauthorized packages, local-path and metadata disagreement, and symlink-mediated checkout leakage (`helper-control-results.json`). These mocks exercise integrity decisions, not compilation. The actual source/archive/Wasm results remain the separately recorded production qualifications.

## Nonblocking existing documentation follow-up

The moved `schema.ts` retains obsolete evaluator wording from `internal.ts`: `SchemaOrigin` says prepared contracts retain no source text, and `Schema.prepare` describes older blanket stand-in/unevaluated preparation refusals. The original semantic Plan can retain original sources and now handles more such schemas. This predates the split and does not invalidate the move; correct it alongside the already planned evaluator documentation update rather than treating those statements as current lifetime/capability guarantees.

No behavioral blocker found in the reviewed changes. Original snapshots, exact source hashes, independent probes and results are retained here.

Follow-up at `511a1bf`: the reported retained-source and obsolete blanket-refusal comments were corrected in `schema.ts`, without behavioral edits. Reviewed wording and its hash are retained in `corrected-schema-documentation.ts` and `documentation-correction.json`.
