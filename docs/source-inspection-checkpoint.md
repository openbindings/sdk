# Source-inspection foundation checkpoint — 2026-10-11

Implemented on `codex/sdk-source-inspection`, based on SDK
`fb6e647ae5c3d5954ed2d56c42c794bea06cb37f`. This is the first optional role companion,
not completion of the capability foundation or OpenAPI adapter. The Rust API remains
provisional. No packages have been published. The initial implementation was
`52ab92d`; a bounded repair pass followed three independent GPT-6 Astra xhigh
reviews covering the provider API, consumer recovery, and role contract.

## Delivered

- `openbindings-source-inspection`: provider trait, checked `Inspector`, inventory,
  limitation, target, exact binding fragment and refusal types.
- Complete and partial inventories; exact per-job support; source carrier admission
  before provider work; cancellation; checked author-key assignment.
- Bounded exact result encoding. Rust callers use owned/retained values directly.
- Independent catalog provider and authoring consumer using only public dependencies.
- Pinned role-schema fixture and behavior tests, including restrictions expressed
  only in role prose.
- Packaged external-consumer qualification and a TypeScript/local async Wasm probe.
- Replay/reference/package inventory integration and CI checks for the new capability.

Core code, core dependency declarations, binding specifications and OpenAPI client
code are unchanged. The Source Inspector draft was clarified on interfaces branch
`codex/source-inspection-identities`, commit
`8ff5d9d913cb525dacb347a29e534af278ed88f6`. The new companion depends on core, Serde and the
already-used public JSON serializer with RawValue support. Evaluator and Tokio are
test-only dependencies. Kind interpretation and acquisition remain provider-owned.

## Consequential decisions

1. **Checked facade over provider hooks.** Implementers supply `SourceInspector`;
   consumers use `Inspector`. This makes refusal-before-work and returned-carrier
   validation a consistent SDK behavior without claiming to police a provider's
   source-specific truthfulness.
2. **Binding fragments precede authoring keys.** A result cannot assign operation
   or source keys through typed fields; illegal extension injection is rejected.
   `bind` supplies the caller's keys when creating the core binding.
3. **Associated futures preserve host choice.** Native `Send` and local non-`Send`
   implementations share the contract. The cost is a provider associated type and
   no `dyn SourceInspector` interface. Runtime registry erasure remains unneeded.
   `&P`, `Arc<P>` and `Rc<P>` forward all provider hooks without requiring `P: Clone`
   or boxing its future. A checked future is `Send` when the provider is `Sync`
   and its associated future is `Send`.
   The provider receives an owned `WorkControl` sharing the caller's state;
   consumers still pass `&WorkControl`. A private generic future helper preserves
   `Send` for stack-borrowed providers without imposing an unintended `'static`
   lifetime. Admission and provider work remain lazy until the first poll.
4. **No second JSON value model.** Retained core `JsonValue` carries opaque data.
   The first test exposed that core's checked Serde conversion rejects nested exact
   values by design. `SourceInspection::to_json` now supplies a bounded RawValue-aware
   encoder instead of weakening that core boundary.
5. **Unique identity within one inspection.** The draft role now requires unique
   `sourceRef` values using exact whole-string equality without normalization.
   Consumers reject duplicate identifiers, including identical repeated rows;
   they do not silently merge or deduplicate them. A producer must not reuse an
   identifier for distinct target-scope units in the same observed source state.
   This does not make identities global or promise correspondence after source
   replacement. SDK, exact encoding and ordinary Serde enforce the same rule.
6. **Validation is independent of the chosen serializer.** Ordinary Serde and the
   exact encoder refuse the same invalid inventories and standalone target fields.
   Only the exact encoder applies its JSON byte/node/depth limits. Private wire
   views share the field mapping without another full inventory validation pass.
7. **Recovery retains evidence.** The external application's outcomes keep partial
   inventory/limitation data, editable drafts and document assessments. Typed
   inspection refusals remain available. A deliberate selection from a partial
   inventory does not upgrade completeness. `Ready` means the authored document
   passed core conformance assessment, not that source correspondence was proven.
   Its small test kind projects only boolean input hints; unestablished optional
   framing is left absent without losing admitted targets.

Native error classifications and cancellation remain SDK/host facilities, not
added wire fields. The role permits optional limitation evidence even on exhaustive
results; the native model preserves that.

See the [crate's role mapping and implementer example](../crates/openbindings-source-inspection/README.md)
for the precise API and behavioral obligations.

## Verification

The following passed locally using Rust 1.99.0 and the pinned dependencies:

| Check | Result |
| --- | --- |
| Core without default features | Builds independently |
| Companion tests | 13 boundary tests, 1 pinned-role output-schema test, 1 doctest pass |
| External consumer | 6 tests and runnable recovery example pass from checkout and extracted Cargo package sources |
| Native async | Borrowed/shared non-`Clone` provider and stack-borrowed unboxed future satisfy `Send`; real suspension exercised |
| Local Wasm async | Rust provider retains `Rc` across a JavaScript Promise and delivers inventory through actual Wasm in Node |
| TypeScript | Strict compilation against generated declarations; runtime probe passes |
| Value fidelity | Absence/null, Unicode and integer `9007199254740993` survive exercised paths; Rust encoding also preserves opaque duplicate names and escaped UTF-16 |
| Authoring | Caller-selected target becomes an assessed conformant OBI with author-owned keys; partial inventory and repair evidence remain available |
| Cancellation | Unpolled calls do not invoke the provider; pre-cancelled calls refuse work; two pending local calls both wake and drop their provider futures |
| Tooling | Rustfmt, companion/external Clippy, missing-doc and broken-link rustdoc checks pass |
| Replay reliability | Two regressions cover stale success receipts after failed reruns and refreshed extracted-source timestamps |
| Role amendment | Canonical order, shared schemas, interface profile and core alignment checks pass in the interfaces repository |

Replay uses `tools/verify-source-inspection.py`; `--archives --wasm` enables package
and host qualification. Generated receipts and command logs are under
`target/source-inspection-qualification/`. They record source cleanliness and archive
hashes. A failed rerun removes the previous success receipt. Extracted sources get
fresh timestamps so Cargo cannot mistake a newly packaged API for an older build
at the same path. This is local evidence; remote CI has not run for this branch.

All three reviewers closed their original findings after focused repair review.
The provider reviewer also reproduced the borrowed-future lifetime issue and
verified the minimal fix with temporary compile probes. These sign-offs cover the
inspection slice; they do not establish performance or adoption grades.

The existing vendored JSON dependency emits a deprecated-API warning under this
toolchain. The new code's warnings-as-errors Clippy and rustdoc checks pass. No
vendored source was changed to silence the existing warning.

## Limits and next slice

The checks establish this extension seam, not broad performance or adoption grades.
There is no new public TypeScript role facade, JavaScript-authored provider bridge,
browser acquisition, Cloudflare qualification, remote protocol transport, OpenAPI
adapter, or broad benchmark qualification in this checkpoint. Existing core/bridge
semantics were not changed, so the full historical SDK corpus was not rerun.

Next is the synthesis companion using the same source/value ownership principles.
Its external provider should also inspect, so shared target identity, selective
synthesis, coverage and truthful omissions can be tested against this working API.
Do not require synthesis to consume an inspection result or force either role to
implement the other. Shared source analysis belongs privately in the provider.
