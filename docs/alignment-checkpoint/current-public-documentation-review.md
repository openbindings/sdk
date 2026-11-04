# Current public documentation and terminology review

**PASS for this bounded cleanup at the hashes in [source-hashes.json](source-hashes.json).** The reference is SDK `0b9dc0466d77623f2ed61b43c02a00ab98e75710`. The review covers the 52 changed files in [reviewed.diff](reviewed.diff), plus the appended prepared-numeric design corrections. No production file was edited by the reviewer. This does not close the original-engine, complete compiler allocation, iterative-depth, diagnostic, API-audit, or performance gates.

## Findings and resolutions

- **R-D1, P2 — missing required meta-schema was obscured. Resolved.** The initial Rust/TypeScript `NoVerdictReason` documentation said missing content did not itself refuse preparation. Ordinary evaluation references can be retained and masked by independent assertions, but missing content required to admit a selected custom meta-schema prevents preparation. The existing `custom_profiles.rs::meta_reference_missing_bytes_remain_a_structured_preparation_failure` is an exact counterexample: it requires `PreparationFailureKind::Resolution` with `ResourceUnavailable` and the original missing URI. Final Rustdoc, JSDoc, and the TypeScript guide now state both boundaries.
- **R-D2, P2 — historical stack measurements looked like current whole-engine guarantees. Resolved.** `docs/support.md` retained the old cross-platform, 39-shape compiled-lane measurements without a lane/revision qualification. It now identifies these as historical compiled-lane evidence, links the historical performance record, and denies a uniform depth guarantee for the mixed original/compiled engine. It retains the separate current small-stack regression and host support information.
- **R-D3, P3 — pattern accounting wording lost normalization. Resolved.** The first replacement said every distinct pattern value contributed, while the qualified retained-readings implementation deduplicates by normalized form. Both Rust and TypeScript limit comments now identify distinct normalized forms of pattern values and separately counted `patternProperties` names.

The initial diff and hashes are preserved as `initial-reviewed.diff` and `initial-source-hashes.json`; final evidence does not overwrite the finding state.

## Production and test disposition

All 24 changed Rust files become text-identical to the checkpoint after removing whole-line comments and applying the exact intended “value contract” → “schema” diagnostic substitution and one renamed internal test function. [rust-diff-classification.json](rust-diff-classification.json) records that check. Manual review confirms no changed verdict, refusal code, arithmetic, resource limit, control branch, ownership operation, or oracle in those files. Diagnostic messages remain fixed/redacted; changed tests continue asserting exact messages and the unchanged arithmetic, status, location, and completeness facts.

The TypeScript production behavior change is removal of the obsolete `diagnostics`-specific message branch. Unknown preparation fields still pass through ordinary `workSignal` validation and throw `SdkError` with `invalid-work-options`. The compile-time interface stays closed. The replaced tests retire the special migration message, not the rejection obligation. Independent controls verify both the old field name and an arbitrary unknown field remain rejected, including `diagnostics: undefined`; unknown evaluation options remain rejected too. No public option or declaration was added.

Example-local variables, status strings, and presentation text now use schema terminology. The examples continue selecting input/output from conformant owners, preparing explicitly, retaining prepared snapshots before asynchronous handoff, and disposing temporary owners. Their status rename from `no-value-contract` to `no-schema` is intentional example cleanup; preserving an incidental example protocol is outside this greenfield task.

The revised public guidance correctly distinguishes parsing, document conformance, schema preparation, and value evaluation; uses immutable edits and current `ExternalSchemas`/`Schema`/`PreparedSchema` APIs; and permits prepared validators to retain original immutable sources. It no longer claims all prepared schemas discard source text or that an unresolved ordinary reference necessarily decides the value. Public budget documentation now separates preparation and evaluation failure, diagnostic incompleteness, caller-selected work limits, and remaining compiler allocation limits. Historical performance prose is explicitly historical. No new whole-SDK qualification claim was found.

The compiled-fixture arena-retention assertions were preserved exactly. Their commentary now correctly limits the no-original-arena expectation to those fixtures; original semantic evaluation is allowed to retain the immutable inventory it uses.

## Independent validation

Using Node 22.23.0 and the actual built SDK/Wasm artifacts:

- **14 public controls passed**: obsolete/arbitrary option rejection, fixed safe failure message, prepared ownership after source-handle disposal, outcome/explanation behavior, cancellation and clean retry.
- **Three negative type controls passed**, with valid preparation/evaluation/explanation calls alongside them. TypeScript rejects the obsolete option and arbitrary unknown fields without relying on its former special error message.
- **Six focused production tests passed** across diagnostic budgets, diagnostic details, diagnostic policy, and examples.
- `git diff --check` passed.

Probe source, logs, exact commands, toolchain identity, and artifact hashes are retained here. [validation.json](validation.json) confirms the tested artifact hashes were unchanged during these checks. The implementer separately reports the full Node 180, types/archive, and both browser 505 + 1,546 lanes passing; those broad runs were not repeated for this wording-only review.

## Appended numeric policy confirmation

The revised `prepared-numeric-operands.md` is consistent with the accepted bounded optional representation. **256 bytes per token**, **65,536 cumulative eager source bytes**, and **1,024 original-node records** are acceptable private optimization thresholds. The node-record cap differs from the review's illustrative operand cap, but each record has at most **13** predicate slots: five numeric, six size, and two applicable contains counts. Thus wrong-kind markers and record/vector/map ownership remain bounded even when no numeric source bytes are charged. The implementation must reserve all of that ownership, not just the parsed token text.

Count-first selection is acceptable while evaluation retains the existing keyword order and demand sites. Oversized operands and entries beyond aggregate/record caps must select original fallback before optional parsing charges. An absent cache record must never mean absent predicates. Orphan counts and inactive vocabulary fields remain inactive. Real attempted construction exhaustion stays sticky; no default cap increases or blanket near-limit-admission equivalence is promised.

Most importantly, all schema-dependent runtime comparison work/allocation remains: a retained Bound still allocates from a large exponent. Instance parsing and multipleOf scratch/domain limits also remain. The two numerical design findings therefore stay addressed by the intended design, with implementation and allocator controls still required. This confirmation does not qualify an implementation that has not yet landed.
