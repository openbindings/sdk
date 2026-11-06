# OpenBindings source pulling

Use `PullResult::keep` to adopt selected added bindings as a validated document:

```rust
use openbindings::{ConformantDocument, WorkControl};
use openbindings_source_pull::{PullLimits, PullRequest, PullResult, PullValidationError};

fn adopt(result: &PullResult, request: &PullRequest, control: &dyn WorkControl)
    -> Result<ConformantDocument, PullValidationError>
{
    result.keep(request, ["getUser.api"], &PullLimits::default(), control)
}
```

Keeping a binding keeps its added operation and `uses`, follows schema references
through added and existing entries, and repeats these rules for each reached
addition until nothing new is reached.
The original stays unchanged, unrelated and proposal-only additions stay out,
and stale proposals are not applied. Keys name entries in the report's `added`;
an unknown or existing binding key returns `RelationshipError::KeptBinding`.
Repeated keys are harmless, and an empty selection returns the original unchanged.

For a complete protocol implementation walkthrough, read
[writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md).

This unpublished companion is being implemented against source-puller version
0.1.0 at interfaces revision `3e3dc453a93b611a8e9ea1813ec63a2216830f45`.
It carries additions to one original document and the report of pulling one
selected source. Kind implementations own source interpretation and projection.
A report is a producer claim, not proof of live availability or fidelity.

The carriers preserve exact JSON, allowed output metadata and absence versus null.
Proposals carry restricted edit authority; retaining unknown metadata does not
authorize copying it into a document. No TypeScript interface API is implied.

`PullRequest` retains a complete `ConformantDocument`, selected source key and
optional shared `Context`. `SourcePuller` supplies borrowed finite work and its
own native operational error through `handle_pull_source`. Import `SourcePullerExt`
to call the SDK-owned `pull_source` method directly on the implementation. It checks source existence before dispatch,
then checks key/alias collisions, selected-source attribution, report coverage,
canonical operation keys and uses labels. It validates a prospective document
with all additions before returning the exact result. The original remains
unchanged and stale proposals are never applied.
Use `pull_source_with_limits(request, control, limits)` for caller-selected limits.
The blanket implementation cannot be overridden by an adapter; explicit calls to
`handle_pull_source` bypass its checks. Local checked failures are `PullError`.

```rust
use openbindings::ConformantDocument;
use openbindings_source_pull::{CancellationToken, PullError, SourcePullerExt, PullRequest, SourcePuller};

async fn inspect<P: SourcePuller>(provider: P, document: ConformantDocument)
    -> Result<bool, PullError<P::Error>>
{
    let request = PullRequest::new(document, "catalog");
    let result = provider
        .pull_source(request, CancellationToken::new()).await?;
    Ok(result.report().is_complete())
}
```

`is_complete` means exhaustive with no implementation-unsupported omission and
no unassessed existing entry. It
does not independently verify that the source was read. A context challenge is
returned to the caller; no context collection, source check, retry or source
operation is performed automatically. Dropping pending work cancels its child
token before dropping the provider's future. On cancellation that future is
never polled again, so it cannot perform async cleanup afterward. The token lets
detached work and synchronous loops stop cooperatively; synchronous `Drop` can
release resources held by the future.

`Display` includes a native provider error's own `Display` message. When the
native error implements `std::error::Error + 'static`, `PullError::source()` leads
to `PullFailure`, then to that native error. Validation failures are sources too.
`Debug` continues to hide provider payloads; choose where to display native errors
according to the application's disclosure policy.

`PullResult::validate_for` also checks an already supplied result and returns the
prospective conformant document. That establishes core conformance, not offered
schema identity behind a reuse claim, faithful execution, live availability,
change judgments or needs hidden in binding data. `keep` performs those same
checks before selecting additions, resolves references once in the full result
(including `$id` and anchors), then validates the selected document. A reference
into an added schema or operation keeps that whole entry. Its iterative walk
visits each reached node and edge once, including cycles.
Apply permitted proposal fields through explicit edits and follow their schema
references and binding data `uses` by hand; `keep` covers added entries only.

`PullLimits` defaults to 32 MiB request/response handoffs, 100,000 indexed entries
and uses labels, 64 million relationship work units and 64 MiB cumulative
relationship-index/edit-metadata reservations. They exclude provider work and
already admitted carriers. Core rebuilding, validation and reference inspection
make a fixed number of passes within their own limits, not the relationship
ledger; cancellation is checked between passes and during inspection. Graph
storage and traversal spend the relationship ledger. Incomplete inspection returns
`LimitExceeded("reference-inspection")`. Edited documents retain the original
document's JSON limits.
Requests read through `from_json` are admitted before caller limits, so byte
consumers should first choose explicit JSON parsing limits. Native operational
errors are not measured as JSON; exact context challenges are.

The work default supports the intended 10,000-item catalog workflow while staying
finite. Its checked B-tree search accounting follows the supported Rust node
fanout. Wider keys, denser relationships or larger catalogs may require
caller-selected limits. These allowances are SDK policy, not specification
requirements or a whole-call memory guarantee.

Binding proposals carry `data`, `input`, `output`, or a nonempty `remove` list
naming the fields the kind would no longer write. Source proposals carry `data`
or `remove: ["data"]`. Schema proposals carry `{schema}`. Omission leaves
a field unchanged; `data: null` sets null. Applying proposals remains the caller's
explicit edit. Added entries carry a nonempty array of `lossy` disclosures when
needed. Exhaustive reports carry no limitation.

`uses` on an added entry names other additions the binding's data needs; on a
binding proposal it names what the proposed `data` needs. A schema reference
shows its own target and is left out. The list is not transitive. Each label is
`<map>/<key>`, names an addition, appears once and does not name its own binding.
Only the kind can establish needs hidden in binding data. Schema proposals have
no `uses` accessor; an additional member with that name is retained as unknown
metadata. Existing binding `references` name what the data refers to in the
input document; follow schema references for what those entries reach in turn.

Qualification includes the 38 pinned pull-report cases, five published examples,
74 independently designed controls and two illustrative kind consumers. The old
inspection/synthesis companions and consumers are removed; full SDK integration
and final performance qualification remain separate work.
