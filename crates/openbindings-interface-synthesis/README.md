# Interface synthesis

## SDK policy and interface scope

The publication describes synthesis results. This SDK additionally checks unique
coverage identities, one evidence entry per emitted dependency, preservation of
every input source, and a binding or dependency for every generated operation.
It derives strict eligibility and source attribution through one checked result.
Those are reference-SDK policies, not obligations of every interface adopter.

`sourceKeys` and `dependencyKey` are optional in the shared output shape. This
SDK always emits the source map and requires dependency keys for represented or
lossy dependency entries. A remote report without them can match the interface
without being a checked native `Synthesis`. The policy corpus is maintained here;
the separate interface examples intentionally allow other implementations.

Optional, provisional Rust API for Interface Synthesizer 0.2. Implement
`InterfaceSynthesizer`; consume it through `Synthesizer`. Providers can also inspect,
but neither role requires the other. No public TypeScript role API is released.

```rust
use openbindings::{DocumentBuilder, WorkControl};
use openbindings_interface_synthesis::*;
use std::future::{Ready, ready};
struct Skeleton;
impl InterfaceSynthesizer for Skeleton {
    type Synthesis<'a> = Ready<Result<SynthesisCandidate, SynthesisError>>;
    fn supports_kind(&self, _: &str) -> bool { false }
    fn synthesize<'a>(&'a self, request: &'a SynthesisRequest<'a>, _: WorkControl)
        -> Self::Synthesis<'a>
    {
        let mut draft = DocumentBuilder::new();
        draft.openbindings = request.openbindings_version.unwrap_or(openbindings::AUTHORING_VERSION).into();
        draft.name = request.name.map(str::to_owned);
        draft.version = request.version.map(str::to_owned);
        draft.description = request.description.map(str::to_owned);
        ready(draft.build().map(|document| SynthesisCandidate {
            document, coverage: CoverageDraft { exhaustive: true, ..Default::default() },
        }).map_err(Into::into))
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

In this SDK, both strict and coverage operations use one provider derivation. Strict synthesis
requires exhaustive accounting with no implementation-unsupported unit. Genuine
exclusions, invalid units and sound lossy framing may pass strict synthesis while
full upstream representation is false. On `Incomplete`, `partial()`/`into_partial()`
retain the checked result without acquiring the source again. Invalid provider
output is separate: `rejected()` preserves the candidate and core/coverage diagnosis.

Requests borrow sources. Results retain exact document/evidence owners. `&P`, `Arc<P>`
and `Rc<P>` forward hooks without requiring provider cloning or future boxing.
A checked future is Send when P is Sync and its associated future is Send. Local
futures may retain Rc or host handles. Owned provider controls share caller cancellation;
unpolled futures do no provider work. Cancellation cannot preempt a blocking poll.

Core assessment checks document conformance, not source semantics. The facade also
checks source preservation, output paths, unique coverage identities, dependency
correspondence and evidence structure. A provider remains responsible for truthful
inventory, sound framing, deterministic observed-state behavior and absence of side
effects. Partial coverage never excuses an unsound binding. Multi-source capability
is provider-specific; it must refuse unsupported composition before acquisition.

Coverage uses source index, scope and source reference as identity, with emitted
path keys included for projections. Represented/lossy dependencies name dependencyKey;
every emitted dependency has exactly one corresponding entry. Equal sources may
share emitted keys when supplied names permit; each input index is still accounted for.
`coverage().source_keys()` retains one emitted key per input in request order and
serializes as `coverage.sourceKeys`. Use it to resolve any entry's `source_index`,
including exclusions and dependencies, after the request is gone. The facade derives
this mapping; providers do not supply it. An unnamed input with multiple equal emitted
sources maps to the lexicographically first key; its binding paths may use another
equal source. Explicit names are preserved.

Every generated operation must be referenced by a binding or dependency, including
in partial results. A missing path produces `InvalidResult` with an
`unrealized-operation` finding at the emitted operation. Dependency-only operations
remain valid. The document core continues to allow unbound authoring contracts;
this synthesis check does not establish kind soundness or a binding-to-coverage bijection.

Only checked success objects serialize. Ordinary Serde requires a RawValue-aware
serializer; bounded `to_json_with_limits` additionally applies byte/node/depth limits.
These limits do not bound existing owners or process memory. Exact numeric tokens,
content presence and opaque values never travel through binary64. Source comparison
ignores JSON formatting/member order but preserves numeric spelling and opaque data.

Interface snapshots and separate SDK policy cases are retained in `tests/fixtures`. Package and narrow
Rust-provider TypeScript/Wasm qualification uses `tools/verify-interface-synthesis.py`.
See the repository synthesis checkpoint for acceptance coverage and remaining limits.


## Interface mapping

| Interface Synthesizer 0.2 operation/field | Rust surface |
| --- | --- |
| `listSupportedKinds` | `Synthesizer::list_supported_kinds` |
| `checkKindSupport` | `Synthesizer::check_kind_support` |
| `synthesizeInterface` | `Synthesizer::synthesize` → `ValidatedDocument` |
| `synthesizeInterfaceWithCoverage` | `Synthesizer::synthesize_with_coverage` → `Synthesis` |
| `openbindingsVersion`, source entries, metadata overrides | Borrowed `SynthesisRequest` / `SynthesisSource` |
| `interface`, `coverage` | Immutable `Synthesis::document()` / `coverage()`; exact envelope encoding |
| Coverage `fullyRepresented` | Derived `SynthesisCoverage::fully_represented()` |
| Coverage `sourceKeys` | Derived `SynthesisCoverage::source_keys()`, indexed by each entry's `source_index` |

`strict_eligible()` exposes the separate strict policy; it is not an added wire
field. Cancellation, error categories, retained rejected candidates and typed core
assessment are native host facilities. Providers must return a candidate, including
when incomplete; the facade alone creates recoverable `Incomplete`. A provider's
forged or forwarded `Incomplete` is refused as `ProviderFailure`, retaining its
original error as a cause rather than trusting another request's partial result.

The provider hook is independent of core assessment and coverage validation. It
must preserve optional content absence, source extensions and metadata overrides.
A whole unreadable source refuses the call; unknown inventory remainder uses a
non-exhaustive report with limitation evidence. Unknown kind support is checked
before provider work. Source-free requests must produce an empty, exhaustive
skeleton with the requested metadata.

The current core can retain unpaired UTF-16 JSON but cannot establish document
conformance for it. Such a provider document yields `InvalidResult` retaining an
`Undetermined` assessment, not a lossy rewrite. Opaque report evidence can still
retain those exact strings. This companion does not upgrade core conclusions.

For end-to-end recovery, see `examples/interface-synthesis-consumer/examples/recovery.rs`
in the repository. Persist the complete envelope when coverage matters. Reloading
JSON retains evidence but does not create a new checked `Synthesis`; editing its
document likewise creates an author-owned claim requiring new validation and,
where appropriate, fresh source analysis.
