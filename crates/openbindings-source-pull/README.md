# OpenBindings source pulling

For a complete protocol implementation walkthrough, read
[writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md).

This unpublished companion implements source-puller version
0.1.0 at interfaces revision `484c9399207de99bd56921cead5f44a7b5740d9e`.
It carries additions to one original document and the report of pulling one
selected source. Kind implementations own source interpretation and projection.
A report is a producer claim, not proof of live availability or fidelity.

The carriers preserve exact JSON, allowed output metadata and absence versus null.
`OmittedEntry` borrows `item`, `existing`, `also_identifies`, `offered`, `through`
and `identifier` through accessors using the interface's field meanings; absent
members stay `None`. Proposals carry restricted edit authority; retaining unknown metadata does not
authorize copying it into a document. No TypeScript interface API is implied.

`PullRequest` retains a complete `ConformantDocument`, selected source key and
optional shared `Context`. `PullRequest::from_json` applies the shared
[SDK admission policy](https://github.com/openbindings/sdk/blob/main/crates/openbindings-context/README.md)
to context and refuses an object anywhere in it that repeats a decoded member
name as `RequestError::Shape("duplicate-member")`.
`SourcePuller` supplies borrowed finite work and its
own native operational error through `handle_pull_source`. Import `SourcePullerExt`
to call the SDK-owned `pull_source` method directly on the implementation. It checks source existence before dispatch,
then checks key/alias collisions, selected-source attribution, report coverage,
canonical operation keys, uses labels and input-document labels in binding
`references` and omitted `existing`, `alsoIdentifies` and `through` fields.
Input-document labels use primary keys, with operation slots required to exist. It validates a prospective document
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

`Display` prints each SDK error's own layer; `Debug` hides provider payloads.
When the native error implements `std::error::Error + 'static`,
`PullError::source()` leads to `PullFailure`, then to that native error.
Validation failures are sources too. Apply the application's disclosure policy
before printing a native cause or a cause chain.

`PullResult::validate_for` also checks an already supplied result and returns the
prospective conformant document. That establishes core conformance, not offered
schema identity behind a reuse claim, faithful execution, live availability,
change judgments or needs hidden in binding data. To keep part of a pull, a
caller inserts the chosen bindings, their operations when added, and what their
`uses` names. Follow schema references to further additions, including references
through existing entries. Each reached added binding brings its own operation
and `uses`. Repeat until nothing new comes in, then validate the edited snapshot.
Apply permitted proposal fields through explicit edits and follow their schema
references and binding data `uses` the same way. The source-pull consumer's
[`using_the_result.rs`](https://github.com/openbindings/sdk/blob/main/examples/source-pull-consumer/src/using_the_result.rs)
shows both as application code.

`PullLimits` defaults to 32 MiB request/response handoffs, 100,000 indexed entries
and uses labels, no relationship work cap (`max_relationship_work: None`),
and 64 MiB of conservative reservations for storage held at once. Each uses
check releases its scratch, and relationship indexes drop before edit metadata
is built. A time bound is the caller's choice through `WorkControl` cancellation
or `Some(work_units)`; units are not a portable duration. See the dated
[measurements](../../docs/performance-boundaries.md#limits-principle-november-2026). They exclude provider work and
already admitted carriers. Core prospective rebuilding/validation uses the
original document's JSON limits and core finding limits, not the relationship
ledger; cancellation is checked before and after those synchronous calls.
Requests read through `from_json` are admitted before caller limits, so byte
consumers should first choose explicit JSON parsing limits. Native operational
errors are not measured as JSON; exact context challenges are.

The default memory and entry limits support the intended 10,000-item catalog
workflow. Checked B-tree search accounting follows the supported Rust node
fanout when a caller selects a work cap. Wider keys, denser relationships or
larger retained indexes may require caller-selected memory limits.
These allowances are SDK policy, not specification
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

Borrow an admitted carrier's exact object with `value()`; use `value().clone()`
when an owned object is needed. On interface carriers, `to_json` means bounded
encoding, as on invocation errors and requests.
