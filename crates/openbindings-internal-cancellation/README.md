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

The scoped registry supports multiple independent registrations, synchronous
relay, isolated local tokens and reentrant wake/drop using only `std` and core.
The invoker-only qualification excludes Tokio. A single abort registration cannot
replace the independent driver, relay and sibling registrations.

A plain `&AtomicBool` would cover today's synchronous checks. The object-safe
`WorkControl` trait also admits caller-owned composite signals and deadlines
without a later signature change; ordinary callers can still pass `&AtomicBool`.
There is no executor, cancellation constructor or async method in core.

Tests cover registration races and both clone windows, replacement, repeated
cancel, reentrant clone-path cancellation/wake/drop, and weak-owner release.
Companion tests cover waking and dropping pending acquisition/provider work,
relay observation during preparation/evaluation/start/poll, cancellation ordering,
and attempt isolation. Discovery's identity test checks all four reexports.
