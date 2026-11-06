# OpenBindings source pulling

This unpublished companion is being implemented against source-puller version
0.1.0 at interfaces revision `5815fb106bddd2c85ff4e93e0077570b3f6b9e74`.
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
canonical operation keys, uses labels and known report dependency closure. It
validates a prospective document with all additions before returning the exact
result. The original remains unchanged and stale proposals are never applied.
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

`is_complete` means exhaustive with no implementation-unsupported omission. It
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
change judgments or dependency closure inside opaque kind data. A caller adopts
a chosen dependency-closed subset or applies permitted proposal fields through
explicit document edits, then revalidates that edited snapshot.

`PullLimits` defaults to 32 MiB request/response handoffs, 100,000 indexed entries
and uses labels, 64 million relationship work units and 64 MiB cumulative
relationship-index/edit-metadata reservations. They exclude provider work and
already admitted carriers. Core prospective rebuilding/validation uses the
original document's JSON limits and core finding limits, not the relationship
ledger; cancellation is checked before and after those synchronous calls.
Requests read through `from_json` are admitted before caller limits, so byte
consumers should first choose explicit JSON parsing limits. Native operational
errors are not measured as JSON; exact context challenges are.

The work default supports the intended 10,000-item catalog workflow while staying
finite. Its checked B-tree search accounting follows the supported Rust node
fanout, rather than a binary-tree height. The measured catalog requires
46,765,436 relationship work units and 48,471,232 reserved bytes; the earlier
20-million work policy refused it. This is an explicit SDK policy revision,
not a specification requirement or a whole-call memory guarantee. Wider keys,
denser relationships or larger catalogs may still require caller-selected limits.

The publication has no proposal deletion representation. Omission means no
change, `data: null` sets null, and null is invalid for input/output schemas.
Removal-only drift requires operational inability or a separate caller-owned
removal edit; it must not become a fabricated current/unmatched report.

Qualification includes the 21 pinned pull-report cases, five published examples,
74 independently designed controls and two illustrative kind consumers. The old
inspection/synthesis companions and consumers are removed; full SDK integration
and final performance qualification remain separate work.
