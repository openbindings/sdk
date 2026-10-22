# Interface synthesis

Turn sources into a conformant OpenBindings interface with coverage describing
what each source contributes. Implement `InterfaceSynthesizer`; call it through
`Synthesizer` for checked results, strict synthesis and retained partial results.
This minimal provider creates an empty interface from a request with no sources.

```rust
use openbindings_interface_synthesis::CancellationToken;
use openbindings::Document;
use openbindings_interface_synthesis::*;
use std::future::{Ready, ready};
struct Skeleton;
impl InterfaceSynthesizer for Skeleton {
    type Synthesis<'a> = Ready<Result<SynthesisCandidate, SynthesisError>>;
    fn supports_kind(&self, _: &str) -> bool { false }
    fn synthesize<'a>(&'a self, request: &'a SynthesisRequest<'a>, _: CancellationToken)
        -> Self::Synthesis<'a>
    {
        ready((|| {
            let mut document = Document::new();
            document.set_openbindings(request.openbindings_version.unwrap_or(openbindings::AUTHORING_VERSION))?;
            if let Some(value) = request.name { document.set_name(value)?; }
            if let Some(value) = request.version { document.set_version(value)?; }
            if let Some(value) = request.description { document.set_description(value)?; }
            Ok(SynthesisCandidate::new(document, CoverageClaim::complete(Vec::new())))
        })())
    }
}
async fn example() -> Result<(), SynthesisError> {
    let provider = Skeleton;
    let synthesizer = Synthesizer::new(&provider);
    let request = SynthesisRequest { name: Some("New interface"), ..Default::default() };
    let result = synthesizer.synthesize_with_coverage(&request).await?;
    assert!(result.coverage().fully_represented());
    let _persisted = result.to_json()?;
    Ok(())
}
```

Providers return an editable `CoverageClaim` in a `SynthesisCandidate::new(document,
coverage)`. Use `CoverageClaim::complete(entries)` to claim an exhaustive inventory
with no limitation, or `CoverageClaim::partial(entries, limitation)` with a
`CoverageLimitation::new(code, message)` for an unknown remainder. Start each
disposition with `CoverageEntry::new` and assign the fields its status requires.
The facade checks the claim against the request and candidate document before
returning `SynthesisCoverage`; the constructors do not establish correspondence or
provider honesty. Provider and result records may gain fields in later versions,
so build them with these constructors and match them with `..`.

Use `synthesize_with_coverage` to inspect the checked document and its coverage.
Use `synthesize` when the application requires the SDK's strict eligibility
policy. On an incomplete result, `partial()` or `into_partial()` retains the
checked result. Invalid provider output has a separate rejected candidate.

## Limits

The SDK validates document conformance, source preservation, coverage identities
and output paths. Two of these checks are SDK policies beyond the published
Interface Synthesizer, not obligations of every interface adopter, and both refuse
output the interface allows. Coverage identities must be unique, so repeated
identical evidence rows are refused as `duplicate-unit`. Every input source must
appear in the emitted document, including one that contributes no target, or the
result is refused as `source-preservation`. Providers remain responsible for
truthful inventory and sound framing under each source's kind. Partial coverage
never excuses an unsound binding. Strict eligibility requires exhaustive
accounting with no unsupported unit; it can still include genuine exclusions,
invalid units or sound lossy framing. Check `fully_represented()` when that
stronger condition is needed.

Sources and requests are borrowed; checked results retain their exact values.
Source equality compares numbers by exact decimal value. Serialization preserves
emitted numeric spelling. `to_json_with_limits` bounds emitted bytes, nodes and
depth, without bounding existing owners or process memory.

Providers may return local futures. Cross-thread calls require a `Sync` provider
and a `Send` future. Cancellation cannot interrupt a blocking poll. This
provisional Rust API implements Interface Synthesizer 0.2 and requires Rust 1.88.
Public TypeScript synthesis is not provided.

## Reference

See `SynthesisRequest`, `Synthesis`, `SynthesisCoverage` and `SynthesisError`.
The [complete consumer](https://github.com/openbindings/sdk/tree/main/examples/interface-synthesis-consumer)
shows source accounting, caller-owned keys and strict-result recovery.
