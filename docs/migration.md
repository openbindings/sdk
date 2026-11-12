# Integrating the current 0.2 SDK

This unpublished SDK follows the pinned 0.2 specification and current interfaces.
Earlier SDK APIs, carriers and implementation arrangements have no compatibility
protection. There are no adapters for the retired inspection/synthesis interfaces or
parallel legacy document and schema models.

Use an exact source revision or verified local archives through
[source replay](REPLAY.md). The registry package named `@openbindings/sdk` may refer
to the separate legacy implementation; a registry install alone does not select
this source API.

Build integrations around the current responsibilities:

| Work | Current boundary |
| --- | --- |
| Read, author and validate an OBI | `Document`, exact JSON, immutable edits and `DocumentValidation`; canonical entries come from `ConformantDocument`. |
| Check caller-facing values | Select the original `Schema`, an evaluator and explicit external resources; retain `PreparedSchema` and distinguish preparation failure from value outcomes. |
| Discover or publish an OBI | The optional HTTP discovery companion, with explicit transport/acquisition policy. |
| Discover kind capabilities or check a proposed source | The kind-description companion's separate description and explicit checking operations. |
| Pull source-derived authoring changes | `SourcePuller::handle_pull_source` and checked `pull_source`, report inspection, prospective conformance and explicit caller adoption. Proposals do not apply themselves. |
| Compare schemas for authoring | The optional comparison companion: normalized identity, directional compatibility and per-operation interface checking, with reusable document contexts. |
| Invoke a binding or operation | `BindingInvoker` with checked `invoke` or `DefaultOperationInvoker` with a caller-selected provider; operation invocation adds declared schema gates. |

The TypeScript facade currently exposes exact documents, schema evaluation and HTTP
discovery. Rust supplies the interface implementations. A fixture's Wasm bridge
does not imply a public TypeScript interface API.

Start with [Rust first use](rust-first-use.md), the
[TypeScript guide](../packages/typescript/README.md),
[schema evaluators](schema-evaluators.md), and [invocation](invocation.md).
The [architecture](architecture.md) explains ownership and module boundaries.
Use current examples and type documentation rather than translating old names
mechanically. Absence of an input/output schema, a present boolean schema, JSON
null, and a preparation refusal have distinct meanings.

This development branch is not a release or a completed whole-SDK alignment claim.
