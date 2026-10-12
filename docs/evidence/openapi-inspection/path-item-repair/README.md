# Bounded Path Item client repair — 2026-10-12

Client commit `fe09522a9b99a6c6b4918e3de0467b5a638afa5e` on
`codex/path-item-resolution`; SDK parent `cf481c699433b5961a0e21c59829c5ff2e7d9fb9`
on `codex/sdk-openapi-inspection`. Product/consumer source hashes in
[measured-subject.json](measured-subject.json) identify the repaired working tree;
the containing SDK commit is the checkpoint. No main/release merge or publication.

## Repair and boundaries

Unknown Path Item members no longer disable recognized methods. Standard-method
conflicts across reference layers retain located evidence and refuse that method,
while unaffected methods remain discoverable, selectable and preparable. All
layers' IDs still participate in uniqueness. Shared-field conflicts, missing
resources, edition boundaries and reference limits retain refusal behavior.

The client owns resolution; the adapter reads its `path_item_conflicts` and applies
the existing 3.1 candidate's target rule. No private resolver, SDK-core dependency,
published-interface change or binding-specification change was added. Synthesis
and invocation remain subsequent adapter slices. Explicit untyped-bundle layouts
and URI userinfo admission remain limitations.

## Independent review

The existing GPT-6 Astra xhigh reviewer found two material issues in the initial
repair: duplicate declarations were charged twice to the operation limit, and
losing-ID bookkeeping scanned historical per-ID indices quadratically. New-record
admission and per-mount hash membership repair them. Maintained regressions cover
exact operation-limit success/refusal and 1,024 mounts sharing conflicting IDs.

The reviewer independently reran all nine focused client tests and verified both
repairs. It found no remaining material issue in ID uniqueness, diagnostic source
ownership, edition recognition, shared-field refusal or bounded conflict metadata.
After the client pin update, it checked package metadata, hashes and receipts:
exact Git client, extracted SDK packages, repaired consumer cases and Node/Wasm
all match. No bounded review finding remains. Performance below was measured
separately; the reviewer did not run concurrent builds or benchmarks.

## Qualification

- Full `dynamic-openapi-client` tests: 366 passed, including doctests and localhost
  HTTP tests; strict all-target Clippy passed. The first sandbox attempt could not
  bind localhost; the completed suite ran with socket permission.
- Nine maintained client regressions exercise direct/referenced unknown members,
  method-local conflicts, all-layer IDs, external source provenance, inheritance,
  shared-field refusal, edition contrast, missing/cyclic/wrong-kind references,
  depth bounds, exact operation limits and many mounts.
- `verify-openapi-inspection.py --archives --wasm` passed: adapter and existing
  inspection tests/doctests, strict Clippy, extracted package consumer (three tests),
  exact Git dependency and core isolation, Rust/Wasm compilation, strict TypeScript
  and the Node host. [Receipt](qualification-receipt.json).
- The consumer inspects a healthy target, authors/reloads an OBI, then prepares its
  corresponding request through the public client. A conflicting method refuses.
  This is correspondence evidence; no adapter invoker is implemented.
- Node/Wasm: exact-number preservation, pre-cancellation, conflict locality and
  1,000 owner-release cycles pass. Both logical-owner deltas are zero.

## Performance

The first follow-up measurement run passed the unchanged registered budgets and
stability criteria. All raw observations are retained in [measurements](measurements/).
Three interleaved process pairs, seven samples per workload, at least 50 ms per
batch; release Rust 1.99 on macOS arm64. No retries or discarded candidate runs.

| Operations | Direct cold | Adapter cold | Direct warm | Adapter warm |
| --- | --- | --- | --- | --- |
| 1 | 6.054 µs | 8.157 µs | 0.142 µs | 0.391 µs |
| 32 | 101.869 µs | 147.366 µs | 3.601 µs | 3.473 µs |
| 512 | 1.585 ms | 2.323 ms | 61.475 µs | 52.158 µs |

Process baseline spread stays below 3.9% (15% limit). Cold ceiling remains
`2*B + 50,000 + 500*N ns`; warm ceiling `2*B + 25,000 + 1,000*N ns`.
All native owner deltas are zero after 1,000 cycles. Counters do not measure heap
or RSS; this does not qualify full browser/Worker/service startup or performance.
The initial first-slice evidence in the parent directory is unchanged historical
evidence; these results apply to the repaired client and adapter source hashes.
