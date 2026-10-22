# Source inspection

Enumerate a source's bindable targets and optional operation framing. Implement
`SourceInspector` in a kind implementation; call it through `Inspector` to check
support, carrier shape, cancellation and returned bindings.

```rust
use openbindings_source_inspection::CancellationToken;
use openbindings::Source;
use openbindings_source_inspection::{
    BindableTarget, BindingValue, InspectionError, Inspector,
    SourceInspection, SourceInspector,
};
use std::future::{Ready, ready};

struct EchoInspector;
impl SourceInspector for EchoInspector {
    type Inspection<'a> = Ready<Result<SourceInspection, InspectionError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == "urn:example:echo"
    }
    fn inspect<'a>(&'a self, source: &'a Source, _: CancellationToken)
        -> Self::Inspection<'a>
    {
        ready(Ok(SourceInspection::complete(vec![BindableTarget::new(
            "value",
            BindingValue {
                content: source.content.clone(),
                ..Default::default()
            },
        )])))
    }
}

async fn select(source: Source) -> Result<openbindings::Binding, InspectionError> {
    let provider = EchoInspector;
    let inspector = Inspector::new(&provider);
    let inventory = inspector.inspect(&source).await?;
    let target = inventory.targets.into_iter().next().expect("echo has one target");
    // A binding invoker takes `target.binding` as it is; a document needs keys.
    Ok(target.binding.bind("my.operation", "my.source")?)
}
```

A target's binding is a `BindingValue`, the by-value binding a binding invoker
receives, so an application can invoke a chosen target directly. To author a
document, it assigns operation and source keys with `BindingValue::bind`, which
returns a core `Binding`. Its structural `BoundaryError` converts into an
`InspectionError` that keeps the boundary code and location, so one error type
covers inspecting and authoring. Missing operation framing is permitted; the
author can supply it before building and validating the final document.

Providers build results with `SourceInspection::complete` or `partial`,
`BindableTarget::new` (with `with_operation_key` and `with_operation`),
`InspectionLimitation::new` and `SupportedKind::new`. These records may gain
fields in later versions, so match them with `..`. `SupportedKind` and
`KindSupportVerdict` are the same types interface synthesis and invocation
reexport, so a provider serving several jobs builds one advisory list. A
`BindingValue` is input data like `openbindings::Binding` and keeps struct-literal
construction; a provider leaves its `operation` and `source` keys absent.

## Limits

The provider owns acquisition and kind semantics. Structural checks cannot prove
truthful support, complete enumeration or sound framing. Partial results carry
explicit limitations. This SDK rejects duplicate `source_ref` rows, a target
binding that carries an operation or source key, and unknown non-extension
carrier fields. These are SDK policies in addition to the published
Source Inspector interface.

Provider futures may borrow configuration and may be local. Cross-thread calls
require a `Sync` provider and a `Send` associated future. Cancellation is
cooperative and cannot interrupt a blocking poll. Inspection does not require
synthesis or invocation.

This provisional Rust API implements Source Inspector 0.1.0 and requires Rust 1.88.
Public TypeScript inspection is not provided.

## Reference

See `SourceInspector`, `Inspector`, `SourceInspection` and `InspectionError`,
and the [complete consumer](https://github.com/openbindings/sdk/tree/v0.2.0-alpha.1/examples/source-inspection-consumer).
The [published interface](https://github.com/openbindings/interfaces/blob/f158f4f345301668ccb8fd555586b04ca823e732/source-inspector/0.1.json)
defines the shared operation meanings.
