# HTTP discovery policy

Origins must first be RFC 3986 URIs, which are ASCII only: a non-ASCII domain
name such as `https://bücher.example` is refused in Rust and TypeScript, so pass
its IDNA A-label form, `https://xn--bcher-kva.example`. An admitted origin is
then parsed and serialized with the same WHATWG `url` implementation as reqwest.
It must have no credentials, resource path, query or fragment; a root slash is
accepted. HTTPS is required except for normalized literal IPv4 127/8 and IPv6
`::1`. Names such as `localhost`, `.localhost` or `127.example` do not
establish loopback. Alternate IP spellings are normalized before admission and
`requested_url` records the normalized URL actually supplied to the transport.

The safe native constructor accepts the application's `ClientBuilder`, installs
the SDK redirect policy last and builds a reusable client. It follows 301, 302,
303, 307 and 308, resolves relative Location values, allows at most ten redirects,
and refuses every HTTPS-to-HTTP hop before dispatch, even to loopback. It also
refuses a hop whose target has the same origin as the preceding URL when that
preceding URL has a different origin from the original request. This prevents
credential replay in tower-http 0.6.11 / reqwest 0.13.5: tower-http restores the
original headers on each hop, while reqwest strips credentials only when the
next scheme, host or port differs from the immediately preceding hop. Thus
A -> B -> B is refused before the second request to B. A -> B, A -> A -> B and
A -> B -> A retain their ordinary behavior. Applications needing the refused
chains must use `Client::with_delegated_policy` and own credential policy across
every hop, as well as the scheme and redirect-limit obligations. A visible
301/302/303/307/308 with Location left unfollowed by the native host is a typed
redirect-policy refusal, including malformed Location that bypassed its callback.
An already-built client is accepted only by `Client::with_delegated_policy`.
Observing a final HTTPS URL cannot prove that all intermediate hops used HTTPS.

The application scopes credentials on redirects. On a cross-origin hop, reqwest
removes only `Authorization`, `Cookie`, `Proxy-Authorization` and
`WWW-Authenticate`; any other credential header, such as an `x-api-key` set
through `ClientBuilder::default_headers`, reaches the redirect target. Carry a
credential in `Authorization` where the service accepts it, or build a client
whose redirect policy refuses foreign-origin hops and pass it to
`Client::with_delegated_policy`, which then owns the per-hop obligations above.

Application policy still controls TLS roots, credentials, proxies and resolved
network addresses. URL admission is not an IP allowlist. For restricted native
hosts, supply a `ClientBuilder::dns_resolver` implementation that filters every
resolved address before returning it. Its connector/proxy policy must enforce
that restriction at connection time, including literal destinations and proxies;
resolver filtering alone cannot restrict literal IPs. The SDK redirect policy
checks URL admission at every hop, while the application must enforce its own
additional address restrictions at every connection. Portable transports carry
the same obligations. Ordinary browser Fetch cannot expose DNS or peer addresses.

The SDK does not enable reqwest gzip, brotli, deflate or stream features. An
application needing decoding enables those features in its own manifest and
configures its supplied builder/client. Decoded bytes remain bounded at 1 MiB by
default; `ClientOptions::default()` selects that byte default; explicit zero and `usize::MAX` are refused. Native
responses yield `ErrorKind::Decode` only when the SDK refuses a response whose
Content-Encoding the host left undecoded. Native body-read errors, including
truncated and corrupt-coded bodies, yield `BodyError` with `ErrorKind::Network`
and retain upstream error and source messages for humans. Reqwest exposes no
documented API establishing a codec failure behind `Response::chunk`; its decode
label alone is insufficient. Identified transport deadlines still yield `Timeout`.
Portable transports may report their own decoding failures as `Decode`.
A transport may buffer larger chunks internally. Only complete bounded 200 bodies
enter validation; oversized or incomplete prefixes never become OBIs.

Every attempt has one thirty-second whole-attempt deadline. It includes connect,
TLS, response headers, the redirect chain, body acquisition and validation.
`AttemptTimeout::After(Duration)` accepts a positive finite override;
`AttemptTimeout::Unbounded` explicitly omits the deadline. Zero is an error.
Native discovery owns one Tokio timer, so it must run on a Tokio runtime with
timers enabled (`enable_time` or `enable_all`), as reqwest's own timeouts do;
without them, Tokio panics when the timer is created. A shorter
application-client timeout is an additional bound and yields `Timeout`. Caller
`CancellationToken` cancellation remains `Cancelled`; caller cancellation wins
when both sources are ready at the same poll, and the first observed terminal
cause ends acquisition.

Portable `discover(origin, &options, &control, &timer, transport)` requires a host `DiscoveryTimer`
providing monotonic time and a wake-capable absolute timer. Its repeatable
callback returns one hop, never follows redirects internally, dispatches the
supplied normalized URL unchanged, delivers decoded bytes or a typed refusal,
and releases work when dropped. The SDK calls it again for each admitted
redirect, including one to another origin, and passes only the URL: the callback
decides which credentials that URL's origin receives, as a browser or reqwest
does for its standard credential headers. Portable results record the URL the SDK
last dispatched as `final_url`. Observable hidden following is a policy failure;
this contract cannot sandbox arbitrary callback code. Local
non-Send futures are supported. The SDK races and drops one host timer per attempt;
redirects and chunk reads never reset it.

TypeScript `discover` owns its timer and AbortController. `timeoutMs` defaults to
30,000; it accepts a positive finite override or `"unbounded"`, and rejects zero.
It uses Fetch `redirect: "follow"` and refuses an observable final HTTPS-to-HTTP
downgrade. A 301, 302, 303, 307 or 308 response that still carries a visible
Location was left unfollowed and is a `redirect-policy` transport error, as in
Rust. Browser per-hop mixed-content blocking in secure contexts and
private-network rules belong to the browser. The SDK cannot observe hidden hops
or enforce its native ten-hop limit in browser Fetch. Opaque and opaqueredirect
responses are typed policy failures because policy cannot be established, never
absence or document violations. A supplied Fetch callback must honor the URL,
redirect mode and cancellation contract. Fetch removes `Authorization` on a
cross-origin redirect but keeps other request headers, so a custom credential
header the callback adds reaches every followed origin. A standard TimeoutError
caller signal is a timeout; ordinary abort is cancellation.

Timers cannot preempt a blocking Rust poll or synchronous Wasm validation. Each
path checks monotonic time and cancellation at validation boundaries and discards
late success. A hard CPU latency bound requires isolated validation and a
responsive supervisor, such as a browser Worker. Complete bounded bytes and
observed metadata survive validation-boundary expiry.

Only 404 means absence and 401/403 mean gated discovery. Other statuses remain
explicit. An observed status is committed before best-effort cleanup; timeout or
cancellation stops cleanup without replacing the status. Native HTTP/1 cleanup
has a 2 KiB / 100 ms bound, further limited by the attempt's remaining time.
Dropping other bodies closes response state. TypeScript clears timers and
listeners, cancels readers/late bodies and releases locks without awaiting an
unbounded cancellation promise. No response cache or global client is installed.


## Publication

Publication retains conformant immutable bytes. GET serves those bytes; HEAD
returns the same headers without a body. Other methods return 405 with
`Allow: GET, HEAD`; other paths return 404. Pass the route path without query or
fragment and do not decode a reserved `%2F` into `/`.

CORS defaults to `AllowOrigin::Any`. Select `Omit`, `Null` or `Exact` explicitly
when needed. Exact origins must use canonical ASCII browser serialization.
Authentication, credentialed CORS, preflight and request-dependent origin policy
belong to application middleware.
