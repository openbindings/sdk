# Evaluation budget terminology — bounded implementation PASS

Reviewed the stable SDK diff over `4e4cdfb`, against the nine frozen design groups. No SDK source was edited by this review. The implementation qualifies as an isolated naming/documentation change with the explicitly frozen private benchmark translation. No open correctness or adapter finding remains. Final rebuilt-host, package/consumer, whole-source and performance qualification remain separate; no timing conclusion follows from this receipt.

## Source and contract audit

Thirty maintained files change. Eighteen normalize exactly to their base bytes after replacing the two new Rust/serde and TypeScript spellings with the old spellings. The remaining changes are the reviewed API descriptions, formatting, added boundary/rejection controls, the private adapter translation/tests, and the two equivalent let-chain observation cleanups. All six JSON fixture files preserve complete structured contents after field-spelling normalization: five change keys only; `semantic-limits.json` also updates that name in its maintained explanatory adjudication. No schema, value, limit number, required outcome, historical-required field or diagnostic code changes.

The actual Graph work/reservation methods, accounting sites, arithmetic, control calls, quota defaults, verdict/Explain ordering and meta evaluation path are unchanged. `semantic/mod.rs` changes only accesses and existing test field spellings. `Limits` retains serde defaults and `deny_unknown_fields`, with optional work `None` and reservation 50,331,648. There are no aliases. Documentation accurately distinguishes per-invocation evaluation debits from shared preparation debits, immutable pattern footprints and physical allocator/process memory; required selected-meta evaluation may produce a PreparationFailure when either relevant allowance is exhausted.

The old-spelling inventory is saved. Remaining occurrences are deliberate rejection tests, the frozen benchmark carrier/generator, or explicitly historical performance/checkpoint evidence. Comparison/source-pull budgets are untouched. The TypeScript source maps the new camel-case names through the unchanged encoder; the new raw-Wasm and TypeScript rejection tests were statically reviewed. Actual execution of those rebuilt-host tests belongs to the parent's qualification, not the private native run below.

## Independent current/minimum Rust checks

Fresh isolated evaluator copies compiled on Rust 1.99 and 1.88 against the unchanged dependency set. Both pass **27 targeted tests**, without a workspace/corpus rerun:

- Eighteen default/domain round trips per toolchain cover omitted fields, work null/zero/positive, native u32-plus-one/usize maximum, and reservation zero/positive/usize maximum. Full default serialization retains every other default. Old-only/mixed/current-invalid objects reject as required; null reservation is rejected.
- Actual ordinary value evaluation uses the renamed fields: work zero yields `evaluation-work-limit`, reservation zero yields `semantic-analysis-memory-limit`. Funded repeated calls, failure explanations, cancellation and clean retries retain their behavior.
- Actual required custom-meta evaluations were tested cold and with a retained environment. Low evaluation work/storage still become **PreparationFailure::Limit** with their respective unchanged evaluation codes; low preparation allowances retain `preparation-sdk-work-units` / `preparation-sdk-reservation-bytes`. Healthy retries succeed.
- Existing cumulative Explain tests cover exact/one-below work/storage, pre/post-verdict cancellation, enclosing Attempt exhaustion, overflow, concurrency and exact original locations. Decision shared-attempt tests, preparation-budget tests and public interruption/retry tests also pass. No old callback counts were substituted for these live boundary controls.
- The actual copied benchmark adapter and its permanent tests pass. Historical omitted/null/zero/nonzero carrier requests retain their complete observations. New-only and mixed replacement spellings reject; historical depth handling remains intact.

`summary.json` records the test groups and independently observed serialization/meta/carrier rows. Current/minimum observations agree. All copied-source builds/tests passed on the first run. Two audit-driver issues were preserved separately: an initially overstrict key-only JSON assertion encountered the explicitly renamed explanatory prose; the replay driver initially used the wrong existing verifier function name. Both stopped before qualification results and were fixed without changing SDK inputs, assertions or oracles.

## Adapter and complete release observations

The actual `compare.instrument_diff` audit reports **confined**, changed file **`src/sdk.rs` only**, and no forbidden changes. Instrument phase code, allocation counter, build script, manifest, lockfile, fixture generator and all third-party lock identities remain unchanged. The adapter rejects replacement spellings before deserialization even if supplied alone, using key presence (including null/zero), then moves original JSON carrier values without coercion. Existing unknown/malformed-key validation remains. There is no readiness, outcome assertion, resource selection, parsing phase, timed-body or allocation-boundary change.

The frozen pre-rename peer's production hashes match base `4e4cdfb`. The new peer's SHA, reported provenance, computed instrument/SDK digests, root Cargo inputs and source manifest match the actual reviewed tree. Both binary SHAs were checked again after replay.

All **42 unchanged fixtures / 101 mode rows** were replayed with one iteration per request. Each side satisfied its unchanged expectation, and **all 101 complete observations are exactly equal**, with zero process/assertion errors. Request objects, resources, numeric allowances, modes and expected outcomes were not rewritten. Timing fields emitted by the instrument are retained only as raw output, not interpreted as measurements or performance acceptance.

- Before binary SHA-256: `7c1e35ef1e9581876a5e5c1c1c7bbaa446dcf2900d70d2f28db67821fdae081f`; instrument `0464ed6974956429`, SDK `4238ea9c175d4547`.
- After binary SHA-256: `c96fe60f172b3db9b4b043f639389e0d9f2620a6c475e25b831ad7947d30f13d`; instrument `3e82ba886b9ac89e`, SDK `d8430ea45d8da187`.

The instrument identities intentionally differ. This receipt does not permit an identical-instrument incremental timing helper to bypass its guard. Any final matched campaign must bind both actual identities and preserve its existing policy.

## Evidence and scope

`source-hashes.json` pins all thirty changed files against the exact base; `production.patch` and `normalized-extra.diff` make non-rename edits reviewable. `instrument-audit.json` retains the actual diff-verifier output and closure. Private copies, commands, build/run logs and `probe-hashes.json` distinguish this review from production. `release-peer.json`, `release-replay-summary.json`, `release-observations.jsonl` and `frozen-fixtures.json` bind the complete request replay. `coverage.json` maps EB-01–09 and distinguishes native execution from parent host qualification.

This is bounded approval of the terminology implementation and benchmark adaptation. The parent's full native/lint/required-corpus results are additional evidence, not claimed as independent reruns. Rebuilt Wasm/Node/type/reference/package/consumer and final performance results should be attached by the parent before the corresponding broader gates close.

## Final standalone-tool lint refresh

After the first approval, standalone tool lint required two pre-existing nested `if let` pairs in the benchmark adapter's NoVerdict and Undefined observation branches to become short-circuit let chains. `tool-lint-only.patch` and `tool-lint-source-check.json` prove these are the only changes since the first reviewed adapter. The location-object test still precedes `remove("source_uri")`, and `insert("resource", uri)` still occurs only after both succeed. No callback, accounting, phase, owner, output mapping or dependency changes. The SDK digest remains `d8430ea45d8da187`.

The first 101-row replay, source/adapter audit, summary and review are preserved in `before-tool-lint/`; the parent preserved its binary as `candidate-before-tool-lint` and manifest as `release-peer-before-tool-lint.json`. Existing 27 current/minimum SDK checks were not needlessly repeated: their SDK sources are unchanged, and the syntax-only adapter change is now exercised by the final release replay.

The final peer (`instrument 3e82ba886b9ac89e`) again passes **all 101 mode observations from 42 unchanged fixtures**, identical both to the frozen pre-rename peer and the first renamed peer. Request hashes and complete observations agree across the two replays. Final binary/source/root manifests were verified before replay and binary SHAs afterward. The actual instrument-difference audit remains confined to `src/sdk.rs`, and all other reviewed source hashes remain unchanged. Warnings-denied standalone tool lint passed in the parent's lane.

Parent-reported broader qualification now includes rebuilt Wasm, type/reference checks, Node 189/189 (local-listener permission failures separately rerun with access), all four consumer-compat configurations, installed TypeScript package 352 assertions plus 32 shared-budget assertions, and its already reported full native/lint/required-corpus gates. These are parent results, not independent reruns. No open implementation finding remains in this final bounded receipt; final package/whole-source/performance obligations remain independently scoped.
