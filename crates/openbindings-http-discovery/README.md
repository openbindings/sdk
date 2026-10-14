# OpenBindings HTTP discovery

This optional companion applies HTTP Discovery 0.1.0 at revision
`2f7d754dc2da374058cd517064c17e50f7d95d99`. Acquisition metadata supplies no
schema base URI and does not change document conformance.

Enable `native` for reqwest discovery:

```rust,no_run
use openbindings_http_discovery::{native::Client, ClientOptions, DiscoveryOutcome};
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let builder = reqwest::Client::builder(); // application TLS/proxy/resolver/decoding choices
let client = Client::new(builder, ClientOptions::default())?;
let result = client.discover("https://service.example").await?;
match result.outcome {
    DiscoveryOutcome::Found { document } => {
        let operation = document.parsed().resolve_operation("lookup")?;
    }
    DiscoveryOutcome::Absent => { /* only an observed 404 */ }
    DiscoveryOutcome::Gated => { /* retained authentication headers */ }
    DiscoveryOutcome::Timeout => { /* attempt or shorter transport deadline */ }
    DiscoveryOutcome::Cancelled => { /* caller cancelled this attempt */ }
    other => { /* HTTP status, document, body or transport outcome */ }
}
# Ok(())
# }
```

Origins are parsed and serialized with the same WHATWG `url` implementation as
reqwest. They must have no credentials, resource path, query or fragment; a root
slash is accepted. HTTPS is required except for normalized literal IPv4 127/8
and IPv6 `::1`. Names such as `localhost`, `.localhost` or `127.example` do not
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
enter assessment; oversized or incomplete prefixes never become OBIs.

Every attempt has one thirty-second whole-attempt deadline. It includes connect,
TLS, response headers, the redirect chain, body acquisition and validation.
`AttemptTimeout::After(Duration)` accepts a positive finite override;
`AttemptTimeout::Unbounded` explicitly omits the deadline. Zero is an error.
Native discovery owns one Tokio timer. A shorter application-client timeout is
an additional bound and yields `Timeout`. Caller `WorkControl` cancellation
remains `Cancelled`; caller cancellation wins when both sources are ready at the
same poll, and the first observed terminal cause ends acquisition.

Portable `discover(origin, &options, &control, &timer, transport)` requires a host `DiscoveryTimer`
providing monotonic time and a wake-capable absolute timer. Its repeatable
callback returns one hop, never follows redirects internally, dispatches the
supplied normalized URL unchanged, delivers decoded bytes or a typed refusal,
and releases work when dropped. Observable hidden following is a policy failure;
this contract cannot sandbox arbitrary callback code. Local
non-Send futures are supported. The SDK races and drops one host timer per attempt;
redirects and chunk reads never reset it.

TypeScript `discover` owns its timer and AbortController. `timeoutMs` defaults to
30,000; it accepts a positive finite override or `"unbounded"`, and rejects zero.
It uses Fetch `redirect: "follow"` and refuses an observable final HTTPS-to-HTTP
downgrade. Browser per-hop mixed-content blocking in secure contexts and
private-network rules belong to the browser. The SDK cannot observe hidden hops
or enforce its native ten-hop limit in browser Fetch. Opaque and opaqueredirect
responses are typed policy failures because policy cannot be established, never
absence or document violations. A supplied Fetch callback must honor the URL,
redirect mode and cancellation contract. A standard TimeoutError caller signal
is a timeout; ordinary abort is cancellation.

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

Publication requires proof and retains exact immutable bytes:

```rust
use openbindings_http_discovery::{Publication, PublicationOptions, WELL_KNOWN_PATH};
let source = br#"{"openbindings":"0.2.0","operations":{"lookup":{}}}"#;
let publication = Publication::from_bytes(source, PublicationOptions::default())?;
let response = publication.respond("GET", WELL_KNOWN_PATH);
assert_eq!(response.status, 200);
assert_eq!(&*response.body, source);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Publication defaults to `AllowOrigin::Any`, emitting `Access-Control-Allow-Origin: *`.
`AllowOrigin::Omit` omits CORS; `AllowOrigin::Null` emits `null`, and
`AllowOrigin::Exact(String)` accepts a canonical ASCII browser-serialized tuple origin.
Noncanonical case/default ports and opaque schemes such as `file:` are refused.
Credentials, preflight and request-dependent Origin selection with `Vary: Origin`
belong to application middleware. Pass the exact route path excluding query and
fragment; do not decode reserved `%2F` into `/`. GET serves exact bytes, HEAD
retains GET headers with no body, other methods return 405 with `Allow: GET, HEAD`,
and other paths return 404. Accept never hides the default OBI representation.

Native TLS tests generate a private CA through OpenSSL and run a Python standard
library TLS server, without OS trust changes. Run the downstream feature witness
with `python3 crates/openbindings-http-discovery/tests/feature-unification.py`;
`--graph-only` compiles all consumer modes without binding sockets. The existing
Chromium/WebKit lane runs `test/discovery-browser.mjs` through `test/browser.mjs`
for real CORS, followed redirects, slow headers and slow bodies.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Package
publication and application cutover are separate actions.
