# Binding invocation checkpoint

> Historical checkpoint. Interface authority and SDK-policy distinctions below
> are superseded by [the interface-boundary repair](interface-boundary-repair.md).
> Its tests remain implementation evidence; they do not impose the SDK's policies
> on independent adopters.

This slice adds `openbindings-invocation`, the third optional role companion after
inspection and synthesis. It implements the binding-provider seam and a usable
checked native lifecycle. Generic operation invocation and the OpenAPI kind
adapter are subsequent slices.

The implementation assignment is in the workspace design record
`design/openapi-adapter-development-loop-2026-10-11/binding-invocation.md`.
This repository carries the public API, examples, authority pin, maintained
acceptance evidence and reproducible qualification tools.

## Authority and decisions

SDK starting revision: `6c08a44bf1b9dc412782ea0aff897435d94ff3d9`.
Applied interfaces: `3972eb977f419f792b92fd61d5afa5fba4946b04` on
`codex/binding-invocation-contract`, following `56aadc07aef362d1bbe1f42dd3081f4a750555f6`.
Both invocation roles remain version 0.1.0 and carry core 0.2 values. The interface
repository has no launch marker; these are prelaunch amendments.

The pinned Binding Invoker JSON is in the crate's `tests/fixtures`; its SHA-256 is
`8bad5b42057ff44ec412488b1ec754e5b2330af10a5e3dd3ab7d0625a373102a`.
The related Operation Invoker receives only the shared schema/prose corrections,
not an SDK operation-selection implementation.

| Decision | Applied correction | Maintained witness |
| --- | --- | --- |
| BI-D1 | Admission may wait for capacity; it never acknowledges dispatch. | Full queue parking, opposite-side progress, pending-write cancellation. |
| BI-D2 | Unary and no-output consumers must observe terminal completion. | Output then error, zero vs null; README consumes through terminal. |
| BI-D3 | Supplied unusable context may receive a renewed challenge only before output/effects. | Safe renewed challenge and independent possibly-dispatched failure fixture. |
| BI-D4 | Caller half-close is optional/idempotent; envelope validation continues while live. | First/single open, repeated close, valid late input, malformed late envelope. |
| BI-D5 | `config.value` is an established requirement family; schema accepts object or Boolean. | Shared interface schema checks plus strict point/JSON Pointer/schema carrier tests. |
| BI-D6 | First terminal wins; outputs committed earlier precede that terminal, including cancellation. | Full-queue cancellation, concurrent completion/cancellation and frame ordering. |

The original SDK-owned driver prototype passed its ownership/progress questions,
so no alternate driver API is retained. The driver takes its pinned provider
future out of a short mutex before polling. Scoped waiter entries fan out wakes
without holding state locks. Signal storage uses `Arc<Waker>`: arbitrary waker
clone, wake and last-reference destruction occur outside the mutex.

Default native policy is one queued input, four queued outputs, 1 MiB per value
and 64 KiB per encoded failure/advisory challenge. One pending submission per
owner is additionally possible. The selected-subtree probe demonstrates that a
one-byte value can retain a 1 MiB backing arena; these limits are not RSS or total
memory guarantees. No provider source is serialized or reparsed per invocation.

`Control<F>` is weak at runtime but retains the borrowed invocation's Rust
lifetime. This permits conditionally Send native use and immediate cooperative
future release on local cancellation without lifetime erasure or a mandatory
executor. Consumers needing later observation retain its owned `outcome()`
snapshot. The Node/Wasm bridge uses one scoped host task to keep borrowed owners
and halves together; that is qualification plumbing, not a second SDK lifecycle.

## Acceptance map

The table identifies maintained assertions, not a grade inferred from test counts.

| Cases | Evidence |
| --- | --- |
| BI01 | `examples/binding-invocation-consumer/invoker-only`; verifier's dependency graph excludes inspection, synthesis, HTTP, optional evaluator and executor. |
| BI02 | `tests/lifecycle.rs`: lazy construction/drop, support/listing, unknown/invalid refusal with zero provider calls. |
| BI03–04 | `tests/values.rs`: strict interpreted boundaries, empty binding, missing/null, extensions, exact numeric/UTF-16 values, optional context/durable and redacted formatting. |
| BI05–07 | Lifecycle and external tests: all cardinalities, zero/null, accepted-input drain, provider early close and output followed by portable error. |
| BI08–09 | Split consumer/provider halves, full queues, dropped pending write/read, no busy wake, 2,000 waiter-release iterations, synchronous reentrant wakes and last-waker destruction. |
| BI10–11 | First-terminal behavior, cancellation with full buffers, concurrent poll/cancel, pre-cancel, output abandonment and isolated sibling attempts. |
| BI12 | Native non-Clone borrowed provider and Send assertion; external Rc across Pending; Rust Rc across actual JS Promise in Node/Wasm; retained exact output. |
| BI13–15 | Checked challenges, no-effects counters, post-output fallback, renewed challenge, separate auth-like failure, default/advisory preflight and invalid preflight error-channel challenge fallback. |
| BI16 | `tests/frame_correspondence.rs` and pinned role: complete closed envelope tables, initial payload refusal, live temporal traces and missing inner terminal. Test mapper only. |
| BI17–18 | Independent Catalog execution oracle after inspection/synthesis and explicit binding choice; repeated use, hand-authored requests and distinct scoped identities. |
| BI19 | Delivery/terminal limits, pending-owner/slot unit tests, isolated allocation probe, subtree backing retention, 10,000 completed plus 10,000 cancelled calls. |
| BI20 | Strict docs/Clippy, external version-dependency consumer, extracted Cargo packages, isolation metadata and strict TypeScript with executed Node/Wasm bridge. |

## Review and repairs

Two bounded independent GPT-6 Astra xhigh reviews cover lifecycle/consumer
ownership and contract/provider correspondence. Consequential findings receive
specific regressions and independent verification:

- Waker destruction under the waiter mutex could reenter cancellation and
  deadlock. A public reproducer timed out before the repair and completed after
  it. Maintained tests cover clear, drop and replacement, plus wake reentrancy.
- Preflight could propagate `Err(CONTEXT_REQUIRED)` although its role reports
  requirements only as a successful advisory result. The facade now uses the
  safe generic failure; typed successful challenges remain intact.
- Catalog execution originally selected the first literal ID, losing scope and
  accepting wrong-typed optional fields through defaults. The fixture repair
  preserves `(scope, id)` for executable alternative/projection units, leaves
  default target string bindings intact, and validates known fields before
  effects. This is a test-kind semantic correction, not a new core requirement.

Provider effect truthfulness, target trust, acquisition security and protocol
behavior remain adapter responsibilities. A structurally valid context challenge
is not evidence that credentials should be released or work is safe to retry.

## Reproduce

From the SDK checkout, with the declared Rust toolchain and Wasm/TypeScript tools:

```sh
cargo fmt --all --check
cargo clippy --locked -p openbindings-invocation --all-targets -- -D warnings
cargo test --locked -p openbindings-invocation
cargo rustdoc --locked -p openbindings-invocation --lib -- -D missing_docs -D rustdoc::broken_intra_doc_links
python3 tools/verify-binding-invocation.py --archives --wasm
python3 tools/verify-interface-synthesis.py --archives --wasm
python3 tools/verify-invocation-resources.py
```

The verifiers write commands, source/archive hashes and host results beneath the
shared Cargo target. The resource probe's checked-in initial timing baseline is
host-specific; `--check-timing` is optional. Structural gates always run. Its
instrumented timings include the safe caller executor, allocator counters and
boxed fixture future; direct and checked paths share the same semantic fixture.
Absolute timings and allocation events are evidence about synthetic overhead,
not HTTP parity, browser memory or a production throughput promise.

Node/Wasm qualification covers asynchronous writes/output iteration, input close,
pre/mid AbortSignal, cancellation, disposal, early iterator return, exact JSON
text, null/absent portable error data, outstanding commands and late Promise
settlement. It reports Node explicitly and adds no published TypeScript surface.

Branch commits, remote CI, merging and publication are separate states. This
checkpoint does not claim remote CI or release landing from local qualification.

## Recorded local result

Implementation commit `c69c0fbfb7e270ba7af9cc6d75506973d3be1627` passed the final
extracted-package and executed Node/Wasm run from a clean checkout: 23 successful
commands, 524 unchanged input hashes and 10 package hashes. The
[compact receipt](qualification/binding-invocation/receipt.json) records the exact
implementation revision, scope, checks and resource measurements. Later receipt
commits do not change the qualified implementation inputs.

All 49 maintained crate tests and its doctest passed, together with six external
three-role tests, the invoker-only consumer, strict Clippy and Rust API reference
gates. The modified synthesis fixture also passed its previous extracted-package
and Wasm/TypeScript qualification. Both independent Astra reviews verified the
consequential repairs and reported no remaining material finding in their scopes.

The final synthetic run measured about 1.3 microseconds for zero-value completion,
2.3 microseconds for one-byte unary echo and 4.3 microseconds for 64-KiB unary echo.
These include measurement and fixture overhead. All fixed resource gates and the
initial same-host timing budgets passed; 10,000 completed and 10,000 cancelled
calls left zero additional live allocations, bytes or provider guards. Node/Wasm
cleanup ended with zero live drivers, providers, commands and tokens.

The checkpoint is committed on feature branches. Remote CI, merging, website
updates and package publication have not been performed by this assignment.

## Focused foundation review repair — 2026-10-12

A subsequent whole-foundation review reproduced two gaps. Commit `d1c4532`
repairs both while preserving the architecture, public API and role authority:

- Shared core `WorkControl` could destroy registered wakers under its waiter
  mutex during replacement or future drop. Waker cloning and destruction now
  happen outside that mutex; a second locked cancellation check prevents late
  registration after cancellation. The maintained destructor regression fails
  against the previous code, and a concurrent registration test checks wakeups.
- Catalog authoring ignored malformed `interaction` fields that its independent
  execution reader refused. Inspection and both synthesis paths now validate
  every unit against the same documented fixture authority. Cross-role negative
  cases and 36 positive mode/scope/disposition combinations guard correspondence.
  No kind-specific interpretation was added to the generic companions.

Two fresh GPT-6 Astra xhigh reviewers independently verified the repairs with no
remaining blocker in their assigned scopes. Cancellation verification included
384 deterministic cases and a mutation removing the second check, which failed.
Catalog verification covered 360 malformed cases, 72 positive cases with 144
calls through synthesized bindings, and 72 valid non-emitted-unit cases.

Core/role suites, the existing discovery and evaluator suites, strict workspace
Clippy, and the no-default core build passed. Invocation and synthesis package
qualification passed from clean commit `d1c4532`, including strict TypeScript and
executed Node/Wasm. All resource gates passed; 10,000 completed plus 10,000 cancelled
calls retained no additional live allocations, bytes or provider guards. The
[repair receipt](qualification/binding-invocation/focused-repair.json) records
source/archive identities, independent checks and remaining qualification limits.
This is local branch verification; it does not claim a merge or remote CI result.
