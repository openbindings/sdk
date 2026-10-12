# OpenAPI 3.1 inspection checkpoint

2026-10-12. This is a bounded, unpublished adapter milestone. It is not complete
OpenAPI 3.1 kind coverage, API stabilization, or evidence of invocation/synthesis.
The first adapter implements the existing optional Source Inspector role; no core
model or published role interface changed.

## Consumer path

```rust
use openbindings::{JsonValue, Source};
use openbindings_openapi::{OpenApi, OPENAPI_31};
use openbindings_source_inspection::Inspector;

async fn example() -> Result<(), Box<dyn std::error::Error>> {
let source = Source {
    kind: OPENAPI_31.into(),
    content: Some(JsonValue::parse(r#"{
      "document": {"openapi":"3.1.2","paths":{"/items":{"get":{}}}}
    }"#)?),
    ..Default::default()
};
let inspector = Inspector::new(OpenApi::default());
let inventory = inspector.inspect(&source).await?;
// Applications retain inventory.limitation when exhaustive is false.
let selected = inventory.targets.into_iter()
    .find(|t| t.source_ref == "/paths/~1items/get")
    .ok_or("target unavailable")?;
let binding = selected.binding.bind("catalog.lookup", "catalog-source")?;
Ok(()) }
```

[The independent consumer](../examples/openapi-inspection-consumer/src/lib.rs)
completes document construction, conformance assessment, serialization and reload.
Its tests separately establish target correspondence through the public client for
both direct and referenced mounts. They also construct a binding without any
inspection suggestion. Core conformance alone does not prove OpenAPI correspondence.

For supplied reference bytes, construct `Options::default()`, add client
`ResourceInput::at(uri, bytes)` values to `resources`, and call `OpenApi::new(options)`.
Use a new immutable provider configuration to recover from missing resources or
change budgets. The original provider continues to describe its original snapshot.
The location-only form needs entry bytes supplied as `content.document` in this
slice; there is no acquisition service or lookup of the entry among reference inputs.

## Pins and interpretation decisions

- SDK foundation: `3bdc5c7773a91494b7ebb23f7a660fdd0f14c4e7`.
- Independent client: `7cec696f19259cffd411c96d4c50f68c4c313558`, exact Git dependency.
- Published role revision: `f158f4f345301668ccb8fd555586b04ca823e732`.
- SDK-applied core: `2f7d754dc2da374058cd517064c17e50f7d95d99`, unchanged.
- Kind candidate starts from the 3.1 document at that core pin; the companion spec
  branch removes only the last, count-dependent sentence of §2 (AD07 below).
  Resulting spec commit: `63e27e11d0f28028b794b61773e17b92e59ed63c`.

AD06 is settled for inventory: after the client's successful Path Item resolution,
a recognized 3.1 method containing an object identifies a mounted operation target.
It does not certify every subordinate declaration or every possible invocation.
No effective-description call, credentials, server choice or schema projection is
required for this task. Primitive method declarations are not Operation Objects.
Unestablished mounts yield partial results with evidence; `discovery_complete()`
is not used as a validity oracle. Method fields come from the resolved chain,
not the client's cross-edition raw operation inventory.

AD07 removes the candidate's rule that makes a source invalid merely because it
declares operations but has no usable target. Adding/removing an unrelated final
neighbor should not change what another declaration means. Representation/root/
version checks and defect locality remain. A complete empty inventory therefore
works for empty and definitively malformed-only operation sets. Optional diagnostic
reporting or rejection algorithms are not prescribed. Other version drafts remain
for their own necessity review; this is not a silent family-wide edit.

Duplicate operation IDs remain an upstream authoring defect. They do not alter
mounted-pointer selection, which does not need that ID. Both distinct targets are
preserved, with advisory names; no normalization or winner selection is imposed.
See [OAS 3.1.2 Operation Object](https://spec.openapis.org/oas/v3.1.2.html#operation-object)
and its [Link Object distinctions](https://spec.openapis.org/oas/v3.1.2.html#link-object).

The necessity audit adds no kind/interface rule. It deletes one unnecessary draft
restriction. Cache layout, ordering, byte limits, cancellation checkpoints and error
codes are adapter product policies. Binding `target` uses the candidate's existing
mounted pointer meaning; choosing the same spelling for `sourceRef` is this
provider's convention. No framing or idempotency is inferred.

## Maintained acceptance and known gaps

The eleven readiness seeds and their original expectation/hash manifest are retained
under [tests/fixtures](../crates/openbindings-openapi/tests/fixtures). The manifest
is historical readiness evidence; current executable dispositions are in
[inspection.rs](../crates/openbindings-openapi/tests/inspection.rs).

| Cases | Current evidence |
| --- | --- |
| AI01–AI03 | Object/text carriers; escaped/Unicode/absent IDs; local referenced mount; public authoring and mounted selection after reload |
| AI04 | Partial missing-resource inventory, actionable resource URI, supplied-resource recovery, unchanged old provider and no invented base |
| AI05–AI07 | Malformed neighbor locality; exact values and opaque dialect carriage; YAML through client loader; no framing |
| AI08–AI10 | All three other family kinds declined, editions mismatched under 3.1 refused; source fixtures retained |
| AI11 | Absent/null/unknown/duplicate carrier fields, unavailable entry bytes, malformed location, future patch, wrong edition/kind, empty and malformed-only inventories |
| AI12 | Duplicate IDs preserve both pointers; borrowed/Arc native and Rc Wasm providers; pre-cancellation; input/result/client bounds; retained output; independently authored OBI |
| Review regressions | No additionalOperations target leakage; exact openapi member required; partial diagnostic budget; URI userinfo classified as client limitation |
| Structural/resources | Warm cache avoids preparation/serialization; content/base changes replace it; immutable resources/limits; 1,000 native and Node/Wasm prepare/use/drop cycles |

Known client limitations are minimized in `unknown-path-field.json` and
`method-conflict.json`. The client rejects unknown Path Item fields and rejects
entire referenced mounts on overlapping method fields, whereas the candidate
ignores unknown fields and preserves nonconflicting methods. The adapter reports
those paths as incomplete analysis and preserves unrelated targets. Explicit
layouts may also be required for untyped resource bundles. URI userinfo is a
client admission limitation (`unsupported-location`), not invalid kind content.

These are recorded client follow-ups, not exclusions added to the kind. No private
resolver was written to work around them. Full 3.1 qualification remains open.
Synthesis, binding/operation invocation, transforms, other editions, automatic
retrieval, production TypeScript packaging and browser/Worker/service qualification
remain subsequent slices.

## Verification and performance

Run from the SDK with Rust 1.99, cached dependencies, wasm32 target,
wasm-bindgen 0.2.129, Node and installed repository TypeScript tools:

```sh
python3 tools/verify-openapi-inspection.py --archives --wasm
python3 tools/measure-openapi-inspection.py --output /absolute/new/evidence-directory
```

The first command runs adapter and existing inspection tests/doctests, strict
Clippy, an external consumer from extracted Cargo archives, exact client-revision
checks, core dependency isolation, and a thin TypeScript/Node-Wasm probe. Cargo
normalizes Git dependencies to registry versions when packaging; qualification
explicitly restores the exact Git client with a patch and verifies its resolved
source. This does not establish registry publication readiness.

The second command measures the same generated documents in three interleaved
baseline/candidate process pairs, seven samples each, minimum 50 ms batches.
Generation and initial SDK carrier construction are excluded; first client parsing,
source adaptation, result checks and result destruction are included as appropriate.
Warm adapter calls reuse prepared results; the baseline copies an inventory from
the prepared client. These do different amounts of work, recorded deliberately.

| Operations | Direct cold | Adapter cold | Direct warm | Adapter warm |
| --- | --- | --- | --- | --- |
| 1 | 6.097 µs | 7.939 µs | 0.139 µs | 0.382 µs |
| 32 | 103.036 µs | 148.504 µs | 3.591 µs | 3.479 µs |
| 512 | 1.593 ms | 2.333 ms | 62.515 µs | 52.241 µs |

Medians from the first execution run, Rust 1.99 on macOS arm64. CPU/RAM metadata
queries were unavailable in the sandbox; the readiness run identified this host as
M1 Max/32 GiB. All three workloads pass the pre-registered cold ceiling
`2*B + 50,000 + 500*N ns` and warm ceiling `2*B + 25,000 + 1,000*N ns`.
Baseline process spread remained below 7%, within the 15% stability gate.
No failed candidate run was discarded and no thresholds were changed.
Native and Node/Wasm client/SDK logical-owner deltas are zero after 1,000 cycles.
Those counters and retained-source accounting do not measure total heap or RSS.
No whole-browser cold-start or service performance claim follows from these results.

[Evidence summary](evidence/openapi-inspection/summary.json) and replay receipts
record files, commands and raw measurements. The separate Astra xhigh semantic
and implementation review found four material issues; one repair pass and focused
verification closed them, with no material finding remaining. That review covers
this provisional first slice, not a complete-family approval.
