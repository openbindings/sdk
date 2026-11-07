# Independent G7 finite consumer composition review

2026-11-03: **PASS for the finite native workflow witness.** The complete reviewed working-tree inputs are pinned by `final-hashes.json`, based on `511a1bf`. No production files were edited by this review. Pinned authorities remain spec `c1419c9fc3ae5a00c2b83da710c11bf67dab7934` and interfaces `5815fb106bddd2c85ff4e93e0077570b3f6b9e74`.

This closes the workflow-evidence portion of G7. It does not close G7's final all-gates review, the ordinary compiler allocation gate, whole-SDK/performance qualification, a published protocol-kind implementation, or publication. The design's shorthand “Close G7” must be read with that scope.

## Finding disposition

**R-G7-1 resolved.** Initial descriptions accepted malformed embedded item carriage and arbitrary fixed selectors that the actual readers rejected. The final embedded schema requires object items, scalar string `key`, and present `operation`/`data`; its description expressly defers offered OBI entry admission until pulling. The fixed binding selector is `const:"echo"`. The later exact-string refinement applies the Unicode-mode scalar-string pattern to both embedded item keys and binding selectors. This preserves empty, BMP, astral, newline and control-character scalar strings while excluding unpaired surrogate units. It deliberately does not impose core operation-key grammar on carriage before the separately disclosed prospective document admission.

Independent `description-probe.rs` executes actual KindClient, SourceCheckClient, and evaluator APIs for eight source and six binding-shape controls. Determinate schema answers agree with reader behavior. Four lone-surrogate pattern evaluations return the existing native `lone-surrogate-instance` UnsupportedCapability; **they are not counted as F**. Actual SourceChecker rejects the corresponding non-scalar source keys. This bounded workflow does not depend on pretending the native evaluator can decide those pattern instances.

Separately, `description-unicode.mjs` calls the real archived Rust/Wasm `describeLocalKinds` export and tests its emitted patterns against an independent UTF-16 well-formedness predicate. Node **22.23.0** passes **2,510,370** comparisons: every single UTF-16 unit with surrounding text/newline variants, every surrogate pair, and deterministic longer mixed strings, for both actual schemas. Artifact hashes and exact patterns are retained in `description-unicode-results.json`.

The new fixture-only kind-description dependency is excluded from invoker-only patches and explicitly forbidden in its resolved graph. Inspected current and MSRV archive metadata both exclude it. Production invocation and pull dependencies remain unbroadened by these fixture additions.

## Actual boundary coverage

| Workflow step | Executed boundary and assertion | What it establishes |
|---|---|---|
| List/describe | `KindClient` calls both fixture operations; exact kinds/support keys, `executes:false`, and no source-check increment | This local implementation's disclosed capabilities, not universal kind support |
| Check | `SourceCheckClient` calls the pure fixed/embedded reader; checks increment exactly once and returned source omits request context | The fixture stands behind the returned carriage; source payload remains exact |
| Author/prove | Caller explicitly inserts checked source via immutable `Document::edit`, then validates | A new conformant document revision; the prior draft is unchanged |
| Pull | `PullClient` invokes Fixed/Embedded, inspects complete report and offered additions | The known fixture's observation and checked relationships, not certification of arbitrary producer claims |
| Select/adopt | Caller chooses echo and explicitly edits only its operation/binding; `uses` is empty and the unchosen operation remains absent | A concrete closed subset is adopted; no hidden automatic application |
| Repeat | Another actual pull reports the adopted echo binding current without adding it twice; embedded's unadopted other offer remains an addition | No duplicate adopted path, with intentional selective retention preserved |
| Drift | Caller changes source binding-data revision in a new document; pull reports stale revision 2, while adopted/changed/prospective binding stays revision 1 | Proposal is reported and remains unapplied; source editing and proposal adoption are distinct caller decisions |
| Choose binding | `OperationRequest::binding` explicitly selects `echo.catalog` | Caller-selected binding with operation validation; no implicit preference policy is claimed |
| Context | Actual invocation returns `CONTEXT_REQUIRED`, no output and zero consumed values; a new caller attempt supplies separate context | This fixture challenges before its operation work and performs no hidden retry |
| Validate/invoke | Wrong integer input yields `ERR_OPERATION_VALIDATION_FAILED` and zero consumed values; string yields one equal output, explicit successful terminal and count one | Input gating crosses the real contained `$id`/`$ref`; output uses the same validated operation driver and success requires a terminal |

The source schema reference is not inert test metadata: if ignored, the embedded wrong-input control would incorrectly reach the provider. Both illustrative kinds are exercised, including the no-data source, an embedded source with multiple offers, exact large JSON numbers, source changes, and separate context. The local providers implement only fixture meanings; no published protocol is claimed. Generic source-pull validation is not described as verifying fidelity or truthfulness of arbitrary `report`/`supports`/`check` claims. The counters are direct evidence about these implementations.

Schema-comparison reuse is not traversed by this particular selective journey: first additions target absent operations; repeated pulls account for existing bindings before reuse comparison. That companion remains covered by its separate source-pull/comparison consumers. No comparison execution claim is assigned to this journey.

## Execution and artifact evidence

Independently reran both complete `journey.rs` groups from the extracted archive test binaries on Rust **1.99.0** and **1.88.0**, after comparing each generated consumer's journey source byte-for-byte with the reviewed source. Both pass; logs and binary hashes are in `journey-{alignment,msrv-1.88}.log` and `journey-executions.json`.

Inspected both archive qualification receipts against every reviewed input they record. All hashes match. The current lane records strict Clippy and Wasm; MSRV records Wasm and honestly records omitted Clippy, with lint provided by the matching development-toolchain lane. See `qualification-inspection.json`. These full verifier executions were performed by the parent; the independent review reran the journey binaries and actual Wasm description probe, not the entire packaging workflow.

The **complete describe/check → author → pull → selective adopt → repeat → drift → context → invoke journey is native-only**. Wasm separately exercises actual kind descriptions, the emitted-pattern controls, and the existing partial pull/adopt/invoke flow with exact values and invalid-input refusal. Its output explicitly says “partial Wasm workflow.” These observations do not claim the complete native journey ran on Wasm.

No remaining blocker for this finite workflow witness. Optional strengthening is to compare the whole context challenge directly in this journey; existing external `role_contracts` already tests exact challenge forwarding, and the current journey tests the required effect/attempt boundary.
