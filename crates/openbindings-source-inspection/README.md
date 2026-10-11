# Source inspection

An optional SDK companion for the OpenBindings **Source Inspector 0.1.0** role.
A kind adapter can implement inspection without also implementing synthesis,
invocation, HTTP discovery or a plugin registry. Core document semantics are unchanged.
This is an unpublished, provisional Rust API. No inspection API has been added to
`@openbindings/sdk` yet.

Implement `SourceInspector`; applications call `Inspector`. This code-defined
example kind has one target whose binding content is the source content:

```rust
use openbindings::{Source, WorkControl};
use openbindings_source_inspection::{
    BindableTarget, BindingFragment, InspectionError, Inspector,
    SourceInspection, SourceInspector,
};
use std::future::{Ready, ready};

struct EchoInspector;
impl SourceInspector for EchoInspector {
    type Inspection<'a> = Ready<Result<SourceInspection, InspectionError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == "urn:example:echo"
    }
    fn inspect<'a>(&'a self, source: &'a Source, _: &'a WorkControl)
        -> Self::Inspection<'a>
    {
        ready(Ok(SourceInspection::complete(vec![BindableTarget {
            source_ref: "value".into(),
            binding: BindingFragment {
                content: source.content.clone(),
                ..Default::default()
            },
            operation_key: None,
            operation: None,
        }])))
    }
}

async fn select(source: Source) -> Result<openbindings::Binding, InspectionError> {
    let inspector = Inspector::new(EchoInspector);
    let inventory = inspector.inspect(&source).await?;
    let target = inventory.targets.into_iter().next().expect("echo has one target");
    target.binding.bind("my.operation", "my.source")
}
```

The [external consumer](../../examples/source-inspection-consumer/) implements an
async catalog inspector, chooses a target, assigns author-owned keys, and builds
and assesses an OBI using public dependencies. It distinguishes kind-excluded
records from admitted records the provider cannot inspect. Missing operation framing
is valid; the author can supply it. Assess the final document after authoring.

## Role mapping

Authority: `openbindings/interfaces` at
`0e37f9fb1ce96c8334a45fd5d091952af825be68`,
[`source-inspector/0.1.json`](https://github.com/openbindings/interfaces/blob/0e37f9fb1ce96c8334a45fd5d091952af825be68/source-inspector/0.1.json)
and its README. Role, SDK package and OBI versions are independent.

| Role concept | Rust mapping and responsibility |
| --- | --- |
| `checkKindSupport` | `Inspector::check_kind_support`; one verdict per unique input in first-occurrence order. Provider checks must be pure and deterministic for their configuration. |
| `listSupportedKinds` | `Inspector::list_supported_kinds`; validates listed kinds against support. Omission has no negative implication. |
| `inspectSource` | `Inspector::inspect`, or `inspect_with_control` for cancellation. Unsupported kinds and invalid carrier members are refused before the provider hook. |
| Source carrier | Core `Source`; typed fields establish their types. Non-`x-` additional fields are refused, including shadowed typed fields. Core itself continues retaining unknown fields. |
| Target identity | Nonempty `BindableTarget::source_ref`. Stability and ordering are provider obligations. Cross-provider equality requires a kind-level rule. |
| Author-owned keys | `BindingFragment` omits `operation` and `source`. Facade, serialization and `bind` reject illegal extension keys. The caller assigns keys through `bind`. |
| Completeness | `SourceInspection::complete` or `partial`; incomplete results require a nonempty limitation code/message. Complete empty results are valid. The role also permits evidence on complete results. |
| Framing | Core `Operation`; rejects `idempotent` and shadowed typed members. Sound projection is the provider's claim; conformance belongs to final document assessment. |
| Exact content | `Option<JsonValue>` preserves absence/null, Unicode and exact numeric tokens. Opaque content never passes through binary64. |
| Refusal | `InspectionErrorKind` and stable `code`; unreadable sources are errors. Partiality is successful data with explicit evidence. |

Structural checks cannot prove truthful support, complete enumeration, source
correspondence, sound framing, idempotency or absence of side effects. Qualify those
claims against the provider's actual kind semantics. There is no implicit relevance
filter, acquisition policy, identity normalization or automatic retry policy.

## Async, ownership and serialization decisions

The associated future preserves a native provider's `Send` capability without
requiring it of browser-local providers. Return `Ready` for synchronous work or
`Pin<Box<dyn Future<Output = Result<SourceInspection, InspectionError>> + Send + 'a>>`
for native async work. Omit `Send` for local host handles. This costs one associated
type in implementations; the trait imposes no allocation or executor. It does not
provide `dyn SourceInspector` dispatch. Applications can compose providers with an
explicit enum; revisit erasure against a real registry requirement.

`Inspector` owns a provider and borrows sources during calls. Results own their
strings and retain exact JSON owners. Dropping or replacing a source cannot invalidate
them. Retaining a small subtree can retain its larger backing arena; there is no
automatic cache or implicit document copy.

`WorkControl` reuses core cancellation. Cancellation drops pending work; providers
must check between bounded synchronous units. It cannot preempt a blocking poll or
undo detached work. Credentials, resource budgets and acquisition stay in provider
configuration.

`SourceInspection::to_json()` validates structure, bounds emitted bytes and applies
core JSON byte/node/depth admission limits. `to_json_with_limits` changes these
limits. These are not limits on existing result owners or process memory.
Serialization allocates separately and is unnecessary for Rust-to-Rust use.
Ordinary Serde is also supported with a RawValue-aware serializer (enable `raw_value`
for `serde_json`). Core `JsonValue::from_serializable` intentionally rejects nested
exact values; use `to_json()` for this inventory. Exact JSON text is also the narrow
Wasm probe's host boundary, avoiding JavaScript-number rounding.

The probe exercises a **Rust-authored provider**, a local future and a JavaScript
Promise in actual Wasm. It does not qualify a public TypeScript role API,
JavaScript-authored callbacks, browser acquisition or Cloudflare execution.

## Replay and remaining decisions

From the repository root:

```sh
cargo test --locked -p openbindings-source-inspection
python3 tools/verify-source-inspection.py
python3 tools/verify-source-inspection.py --archives --wasm
```

The probe needs the pinned Rust/wasm-bindgen tools and TypeScript compiler;
`--tsc` accepts an installed compiler path. Archive mode compiles extracted `.crate`
files with version-only consumer dependencies. It allows uncommitted work and
records that fact and archive hashes; this is not a publication receipt.

Cases cover exact support and carrier refusal before work, inconsistent advisory
lists, complete/partial/empty inventories, unreadable sources, author-key injection,
retained exact values, malformed framing, native `Send`, cancellation/drop, encoding
budgets and local async Wasm delivery.

No abstract-interface amendment was needed. Inspection/synthesis identity
consistency, provenance after source replacement, and portable OpenAPI binding
semantics require later adapter work.
