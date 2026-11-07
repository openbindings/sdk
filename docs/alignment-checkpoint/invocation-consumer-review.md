# Independent current invocation consumer review

2026-11-03. **Bounded PASS after R-IC1/R-IC2 corrections.** No production source was edited. The provider/consumer uses current public APIs coherently, the six requested coverage controls have been adopted, and both verifier integrity defects now reject their independent negative controls. This is an external-consumer gate, not whole SDK or ordinary compiler allocation qualification.

## Authority and scope

Core c1419c9fc3ae5a00c2b83da710c11bf67dab7934 and interfaces5815fb106bddd2c85ff4e93e0077570b3f6b9e74 govern the review. Pinned operation-invoker README40–50 specifies resolution/selection, no hidden preference ranking, input/output gating, and exact terminal/context forwarding;80–86 owns the exact resolution/validation codes and their code-only shape. Binding-invoker README24–30 separates binding/source interpretation from operation validation;128–130 makes preflight advisory;139–158 specifies code plus optional exact data;206–208 distinguishes output from terminal success and makes cancellation no rollback promise. No legacy API or diagnostic spelling was preserved as authority.

Reviewed files: examples/invocation-consumer (provider, native workflow, adopted role-contract tests, Wasm/TypeScript bridge and invoker-only package) and tools/verify-invocation.py. `final-source-hashes.json` pins the exact versions and invocation implementation context. At capture, every pinned file matched the completed archive receipt's input hash, and both newly recorded generated-lock hashes matched their actual locks. `root-final-archive-receipt.json` preserves that completed root run rather than relying on a mutable target receipt.

## Consumer authenticity and boundaries

The external manifest uses version dependencies and public traits/types; qualification supplies explicit patches to source or extracted packages. The authoring fixture is a separately built local example package, not a new invocation dependency. The invoker-only package proves invocation needs neither pull/comparison/default evaluator nor an executor dependency; its test manually polls the public channel.

Catalog reads only binding data, source data and the chosen embedded item. It does not inspect operation declarations on the kind's behalf. The two kinds are explicitly finite local examples; this does not claim a general protocol adapter. Source-data absence remains significant for the fixed kind. Input/output JsonValues are forwarded without serde reserialization. The caller chooses adoption of pull additions explicitly; the original document remains unchanged.

Preflight has its own counter and does not consume operation values. A context challenge is produced before the provider reads input or emits output; the subsequent attempt receives caller-supplied context. The echo counter measures the example's actual input consumption/effect boundary, not a universal proof that arbitrary providers have no effects. Provider work is driven by the output owner; send and receive halves are polled concurrently in the workflow. Both native and Wasm helpers await a terminal and reject missing terminals; output alone is never treated as successful completion.

OperationClient uses the original adopted document's schemas. The corrected Wasm rejection code ERR_OPERATION_VALIDATION_FAILED is required by the current interface for a definite mismatch. It is not a historical expectation substitution. The Node probe is a narrow fixture bridge, not a TypeScript role API or browser qualification claim.

## Independent public controls

The initial four workflow groups passed. `probe/tests/independent.rs` adds seven groups; all seven pass through public current APIs. The first six were adopted as production `tests/role_contracts.rs`, preserving the meaningful assertions after formatting:

1. Provider output incompatible with the operation's output schema is withheld, yields data-free ERR_OPERATION_VALIDATION_FAILED, and retains local Direction::Output evidence. The provider consumed one input, so rejection is actually at the output boundary.
2. Operation preflight and live invocation carry exactly the same challenge; caller context includes a huge integer and lone-surrogate opaque value and arrives unchanged. The challenged attempt produces no output/input effect; invocation does not automatically call preflight.
3. Exact ERR_OPERATION_NOT_FOUND, ERR_BINDING_NOT_FOUND and ERR_BINDING_SELECTION_REQUIRED are asserted with absent data and zero provider starts. Ordered selection skips an unsupported kind and follows caller order despite opposite preferences. Explicit binding bypasses support queries and reaches the selected kind's own refusal.
4. A real Embedded pull/adopt/invoke path preserves binding metadata and exact payloads, including huge exponent and lone surrogate, without mutating the original document.
5. A value followed by an exact kind-owned late error stays unsuccessful; the delivered value is retained and error data bypasses output-schema validation.
6. Pre-dispatch cancellation consumes nothing and produces one ERR_CANCELLED terminal; the stream is fused afterward. This makes no claim of rollback after effects.
7. An unavailable external input schema yields data-free ERR_SCHEMA_UNRESOLVED with input evidence and no provider consumption; anyOf:[true,missing-reference] proceeds. This distinguishes inability from definite mismatch and rejects eager whole-graph readiness assumptions. This last optional control remains in the independent receipt unless separately adopted.

Logs: `independent-tests.log` (original4 + initial independent6), `independent-seven-tests.log` (current seven). The shared-target stale-artifact failure is preserved separately: after another archive build, Cargo briefly reused an authoring-fixture artifact with different SDK trait identities. Touching only this review's copied fixture source forced the correct rebuild; the seven passed. This was build-cache interference, not a production semantic defect. The root agent is isolating consumer output lanes separately.

## Verifier findings, corrected

**R-IC1 — P2, resolved: isolated archive package paths were unchecked.** The main consumer checked expected paths, but invoker-only checked only forbidden package names. A controlled metadata injection resolving its core package back to checkout still wrote a passing archive receipt. The common verify_resolution function now checks all source-less packages against expected local paths for both graphs and rejects checkout paths in archive mode.

**R-IC2 — P2, resolved: root-lock preservation was asserted only in a comment.** The first unlocked cargo test legitimately adds the external fixture, but could also select a different registry version/checksum. Previously an injected dep1→dep9 lock/metadata pair still yielded a receipt. The verifier now permits only a subset of root (name,version,source,checksum) identities plus explicitly named version0.0.0 local fixtures; metadata identities must occur in that checked lock. Generated consumer and invoker-only lock hashes are retained.

The initial and fixed verifier sources are preserved. `verifier-controls.py` / `verifier-control-results.json` reproduce the before state. `verifier-controls-fixed.py` / `verifier-control-fixed-results.json` rerun seven controls: valid source/archive pass; checkout resolution in main or isolated archive graphs, registry drift, an unsuccessful command and changed qualification inputs all reject and leave no receipt. These tests inject command outputs solely to test the verifier's gates; they are not fabricated Cargo qualification results.

The inspected real pre-fix runs already resolved the intended package paths and unchanged registry identities, so these findings do not establish that those actual builds used wrong code. They were defects in what the verifier would accept. Fresh package extraction/copy timestamps, normalized manifests without path dependencies, initial receipt invalidation and final input-hash comparison remain useful and correctly ordered.

## Recorded execution and remaining boundaries

I independently ran the existing archived Node/Wasm probe successfully (`wasm-node-replay.log`) and retained its artifact hashes in `observed-archive-wasm.json`. The completed corrected root archive run includes strict current-toolchain Clippy, native10 workflow/role groups, invoker-only, Wasm build/lint, TypeScript and Node checks; copied commands/logs identify exactly what ran. The earlier MSRV archive receipt records Rust1.88 and omitted Clippy accurately; current-toolchain lint is separate. This review does not infer a new MSRV run of later-added controls from that earlier receipt.

No remaining semantic blocker was found in this bounded consumer/verifier scope. The optional seventh witness is recommended for permanent coverage. Shared-target isolation and broader retirement/tooling checks are separate follow-ups. Ordinary user-schema compiled-backend allocation certificates remain explicitly open in the separate backend schedule review; passing this invocation workflow does not close them.
