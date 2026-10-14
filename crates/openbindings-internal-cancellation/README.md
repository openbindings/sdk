# Internal cancellation

Shared implementation for the HTTP discovery, invocation, source inspection and
interface synthesis companions. Applications import `CancellationToken` and
`Cancellation` from the companion they use; all four exports have one type
identity. This delivery package is not a separate recommended application API.

The token has permanent shared cancellation, synchronous observation, and an
executor-independent owned future with a scoped waiter registration. Cancellation
wakes every registered waiter before returning, outside the registry lock. Waker
clone, replacement, destruction and wake may reenter cancellation. Dropping a
future unregisters its waiter. Core supplies only `WorkControl`, `Cancelled` and
an acquire-loading implementation for `AtomicBool`; this crate depends on core,
never the reverse. `check()` returns `Cancelled`, which each consumer maps at its
own outcome boundary.

Invocation retains its separate relay registration before starting a provider.
The relay cancels a fresh attempt-local token synchronously; a separate
registration wakes the driver. Cancelling or dropping that local attempt cannot
cancel its caller or siblings. Applications should use a fresh token for retries.
Synchronous Wasm still needs a worker or a cooperative host yield to observe a
same-thread JavaScript abort.

## Offline primitive comparison (S6b)

The locked graph contains `futures-util` 0.3.34. The supplied offline Cargo home
also contains Tokio-util 0.7.19 and 0.7.20. No dedicated cancellation package was
present. This comparison reads those local manifests and implementations; it does
not claim a network survey or current upstream maintenance audit.

| Primitive | Required behavior | Dependency cost / decision |
| --- | --- | --- |
| Tokio-util `CancellationToken` | `cancel()` marks children and notifies waiters synchronously; `child_token()` isolates child cancellation from parent and siblings. Owned cancellation futures can support the existing relay. | Both cached versions expose `sync` without `rt`; nevertheless the mandatory graph adds Tokio `sync`, bytes, futures-core, futures-sink and pin-project-lite. The invoker-only qualification explicitly excludes Tokio. Retain the existing lightweight registry to preserve that portable dependency boundary. |
| Futures `AbortHandle` / `AbortRegistration` | Synchronous abort flag plus one `AtomicWaker`; the registration is consumed by one `Abortable`. It cannot directly provide independent driver, relay and sibling registrations. | Already locked, but adapting it would still require a waiter/fan-out registry. It does not retire the SDK machinery. |
| SDK scoped registry | Multiple independent registrations, synchronous relay, fresh isolated local tokens and reentrant wake/drop. | Uses only `std` plus core for its synchronous trait; retained here with the invariant regressions. |

A plain `&AtomicBool` would cover today's synchronous checks. The object-safe
`WorkControl` trait also admits caller-owned composite signals and deadlines
without a later signature change; ordinary callers can still pass `&AtomicBool`.
There is no executor, cancellation constructor or async method in core.

Tests cover registration races and both clone windows, replacement, repeated
cancel, reentrant clone-path cancellation/wake/drop, and weak-owner release.
Companion tests cover waking and dropping pending acquisition/provider work,
relay observation during preparation/evaluation/start/poll, cancellation ordering,
and attempt isolation. Discovery's identity test checks all four reexports.
