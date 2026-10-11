# Source-inspection foundation checkpoint — 2026-10-11

Implemented on `codex/sdk-source-inspection`, based on SDK
`fb6e647ae5c3d5954ed2d56c42c794bea06cb37f`. This is the first optional role companion,
not completion of the capability foundation or OpenAPI adapter. The Rust API remains
provisional. No packages have been published and no abstract interface was amended.

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

Core code, core dependency declarations, interfaces, binding specifications and
OpenAPI client code are unchanged. The new companion depends on core, Serde and the
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
4. **No second JSON value model.** Retained core `JsonValue` carries opaque data.
   The first test exposed that core's checked Serde conversion rejects nested exact
   values by design. `SourceInspection::to_json` now supplies a bounded RawValue-aware
   encoder instead of weakening that core boundary.
5. **No portable contract change yet.** Native error classifications and cancellation
   are SDK/host facilities, not added wire fields. The current role permits optional
   limitation evidence even on exhaustive results; the native model preserves that.

See the [crate's role mapping and implementer example](../crates/openbindings-source-inspection/README.md)
for the precise API and behavioral obligations.

## Verification

The following passed locally using Rust 1.99.0 and the pinned dependencies:

| Check | Result |
| --- | --- |
| Core without default features | Builds independently |
| Companion tests | 9 boundary tests, 1 pinned-role output-schema test, 1 doctest pass |
| External consumer | 2 tests pass from checkout and extracted Cargo package sources |
| Native async | Public checked-call future satisfies `Send`; real suspension exercised |
| Local Wasm async | Rust provider retains `Rc` across a JavaScript Promise and delivers inventory through actual Wasm in Node |
| TypeScript | Strict compilation against generated declarations; runtime probe passes |
| Value fidelity | Absence/null, Unicode and integer `9007199254740993` survive exercised paths; Rust encoding also preserves opaque duplicate names and escaped UTF-16 |
| Authoring | Caller-selected target becomes an assessed conformant OBI with author-owned keys |
| Cancellation | Pre-cancelled input avoids provider work; cancellation drops pending local provider future |
| Tooling | Rustfmt, companion/external Clippy, missing-doc and broken-link rustdoc checks pass |

Replay uses `tools/verify-source-inspection.py`; `--archives --wasm` enables package
and host qualification. Generated receipts and command logs are under
`target/source-inspection-qualification/`. They record source cleanliness and archive
hashes. This is local evidence; remote CI has not run for this branch.

The existing vendored JSON dependency emits a deprecated-API warning under this
toolchain. The new code's warnings-as-errors Clippy and rustdoc checks pass. No
vendored source was changed to silence the existing warning.

## Limits and next slice

The checks establish this extension seam, not broad performance or adoption grades.
There is no new public TypeScript role facade, JavaScript-authored provider bridge,
browser acquisition, Cloudflare qualification, remote protocol transport, OpenAPI
adapter, or independent reviewer sign-off in this checkpoint. Existing core/bridge
semantics were not changed, so the full historical SDK corpus was not rerun.

Next is the synthesis companion using the same source/value ownership principles.
Its external provider should also inspect, so shared target identity, selective
synthesis, coverage and truthful omissions can be tested against this working API.
Do not require synthesis to consume an inspection result or force either role to
implement the other. Shared source analysis belongs privately in the provider.
