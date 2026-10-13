# Source inspection

## SDK policy and interface scope

This crate adopts the inspection operation shapes and meanings. Its checked
facade is deliberately stricter: it rejects malformed carriers and duplicate
`source_ref` rows, including identical rows, and applies its documented bounds.
Those are SDK policies, not requirements of the published interface. A provider
using this facade supplies its checked shape; other implementations may normalize
duplicates or choose different recovery. Interface compatibility does not certify
that a result will pass every SDK policy check.

An optional SDK companion for the OpenBindings **Source Inspector 0.1.0** role.
A kind adapter can implement inspection without also implementing synthesis,
invocation, HTTP discovery or a plugin registry. Core document semantics are unchanged.
This is an unpublished, provisional Rust API. No inspection API has been added to
`@openbindings/sdk` yet.

A source inspector enumerates bindable targets and optional operation framing.
Implement `SourceInspector`; applications call `Inspector`, select targets and
assign their own operation and source keys. The SDK supplies no kind-specific
instances; [openbindings/openapi](https://github.com/openbindings/openapi) is a
separate implementation of the roles. This code-defined
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
    fn inspect<'a>(&'a self, source: &'a Source, _: WorkControl)
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
    let provider = EchoInspector;
    let inspector = Inspector::new(&provider);
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
`f158f4f345301668ccb8fd555586b04ca823e732`,
[`source-inspector/0.1.json`](https://github.com/openbindings/interfaces/blob/f158f4f345301668ccb8fd555586b04ca823e732/source-inspector/0.1.json)
and its README. Role, SDK package and OBI versions are independent.

| Role concept | Rust mapping and responsibility |
| --- | --- |
| `checkKindSupport` | `Inspector::check_kind_support`; one verdict per unique input in first-occurrence order. Provider checks must be pure and deterministic for their configuration. |
| `listSupportedKinds` | `Inspector::list_supported_kinds`; validates listed kinds against support. Omission has no negative implication. |
| `inspectSource` | `Inspector::inspect`, or `inspect_with_control` for cancellation. Unsupported kinds and invalid carrier members are refused before the provider hook. |
| Source carrier | Core `Source`; typed fields establish their types. Non-`x-` additional fields are refused, including shadowed typed fields. Core itself continues retaining unknown fields. |
| Target identity and SDK policy | `source_ref` identifies the source-local target. This SDK rejects duplicate references, including identical rows, and expects stable provider ordering; the interface does not impose either policy. Cross-provider equality requires a kind-level definition. |
| Author-owned keys | `BindingFragment` omits `operation` and `source`. Facade, serialization and `bind` reject illegal extension keys. The caller assigns keys through `bind`. |
| Completeness | `SourceInspection::complete` or `partial`; incomplete results require a nonempty limitation code/message. Complete empty results are valid. The role also permits evidence on complete results. `InspectionLimitation::additional_fields` retains additional evidence; typed fields cannot be shadowed. |
| Framing | Core `Operation`; additional members must begin with `x-`, so unknown and shadowed typed members are refused. Sound projection is the provider's claim; conformance belongs to final document assessment. |
| Exact content | `Option<JsonValue>` preserves absence/null, Unicode and exact numeric tokens. Opaque content never passes through binary64. |
| Refusal | `InspectionErrorKind` and stable `code`; unreadable sources are errors. Partiality is successful data with explicit evidence. |

Structural checks cannot prove truthful support, complete enumeration, source
correspondence, sound framing, idempotency or absence of side effects. Qualify those
claims against the provider's actual kind semantics. There is no implicit relevance
filter, acquisition policy, identity normalization or automatic retry policy.

## Async, ownership and serialization decisions

A checked native call is `Send` when the provider is `Sync` and its associated
future is `Send`. Neither bound is required for local providers. A `Send` provider
future alone does not make a borrowed non-`Sync` provider shareable across threads.
Return `Ready` for synchronous work or
`Pin<Box<dyn Future<Output = Result<SourceInspection, InspectionError>> + Send + 'a>>`
for native async work. Omit `Send` for local host handles. This costs one associated
type in implementations; the trait imposes no allocation or executor. It does not
provide `dyn SourceInspector` dispatch. Applications can compose providers with an
explicit enum; revisit erasure against a real registry requirement.

`Inspector::new(&provider)`, `Inspector::new(Arc::clone(&provider))`, and
`Inspector::new(Rc::clone(&provider))` use the same checked API while letting
independent roles and application methods access one provider's state. Forwarding
does not box futures, add executor requirements or require the provider to be
`Clone`. Applications choose native shared ownership or local ownership as usual.

`Inspector` owns its provider value or handle and borrows sources during calls. Results own their
strings and retain exact JSON owners. Dropping or replacing a source cannot invalidate
them. Retaining a small subtree can retain its larger backing arena; there is no
automatic cache or implicit document copy.

`WorkControl` reuses core cancellation. Consumers pass a borrowed control; the
provider hook receives an owned clone sharing the same cancellation state. Checked
calls remain lazy until polled. Cancellation drops pending work; providers
must check between bounded synchronous units. It cannot preempt a blocking poll or
undo detached work. Credentials, resource budgets and acquisition stay in provider
configuration.

`SourceInspection::to_json()` validates structure, bounds emitted bytes and applies
core JSON byte/node/depth admission limits. `to_json_with_limits` changes these
limits. These are not limits on existing result owners or process memory.
Serialization allocates separately and is unnecessary for Rust-to-Rust use.
Ordinary Serde is also supported with a RawValue-aware serializer (enable `raw_value`
for `serde_json`). Serializing an inventory or standalone target enforces the same
structural validation as checked inspection and exact encoding. Serde does not apply
the exact encoder's byte/node/depth limits. Core `JsonValue::from_serializable` intentionally rejects nested
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
records that fact and archive hashes; this is not a publication receipt. Each replay
invalidates the previous success receipt before work, so a failed rerun cannot
leave a stale success certificate beside its failure logs.

Cases cover exact support and carrier refusal before work, inconsistent advisory
lists, complete/partial/empty inventories, unreadable sources, author-key injection,
retained exact values, malformed framing, native `Send`, cancellation/drop, encoding
budgets and local async Wasm delivery.

The shared role leaves duplicate handling to implementations. This SDK rejects
duplicates through the checked facade and both serializers. That policy neither
makes identifiers global nor proves the provider's inventory truthful.