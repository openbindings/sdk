# OpenBindings HTTP discovery

Discover an OpenBindings interface document at an origin, or publish a conformant
document's immutable bytes through your HTTP server. Core document semantics remain
independent of acquisition metadata.

The applied HTTP discovery companion is version 0.1.0 at specification revision
[`c1419c9`](https://github.com/openbindings/spec/blob/c1419c9fc3ae5a00c2b83da710c11bf67dab7934/http-discovery.md).
Its version and conformance classes remain separate from OpenBindings document
conformance and the SDK package version.

For native discovery, add these dependencies to your `Cargo.toml`:

```toml
[dependencies]
openbindings-http-discovery = { version = "=0.2.0-alpha.1", features = ["native"] }
reqwest = { version = "0.13.5", default-features = false, features = ["rustls"] }
tokio = { version = "1.54.1", features = ["rt", "macros", "time"] }
```

This SDK version is unpublished. Use a local path or Cargo patch for the SDK
crate until it is released. The builder must come from reqwest 0.13, the same
Cargo compatibility line as this crate. A reqwest 0.12 builder has a different
type. Tokio needs a runtime and timers; `macros` enables the entry point below.
Replace the example origin with your server's origin.

```rust,no_run
# #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
# mod example {
use openbindings_http_discovery::{native::Client, ClientOptions, DiscoveryOutcome};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(reqwest::Client::builder(), ClientOptions::default())?;
    let origin = "https://example.com";
    let result = client.discover(origin).await?;
    match result.outcome {
        DiscoveryOutcome::Found { .. } => println!("found a conformant document"),
        DiscoveryOutcome::Absent => println!("absent (404)"),
        DiscoveryOutcome::Gated => println!("gated (401 or 403)"),
        DiscoveryOutcome::TransportError { error, .. } => {
            println!("transport error: {:?}", error.kind);
        }
        _ => println!("discovery did not establish a conformant document"),
    }
    Ok(())
}
# }
# fn main() {}
```

Publish a document without choosing a server framework:

```rust
use openbindings_http_discovery::{Publication, PublicationOptions, WELL_KNOWN_PATH};
let source = br#"{"openbindings":"0.2.0","operations":{"lookup":{}}}"#;
let publication = Publication::from_bytes(source, PublicationOptions::default())?;
let response = publication.respond("GET", WELL_KNOWN_PATH);
assert_eq!(response.status, 200);
assert_eq!(&*response.body, source);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The portable `discover` function instead accepts
a host transport and `DiscoveryTimer`. Judge document bytes acquired some other
way with `openbindings::validate_document`; `Publication::new` and `from_bytes`
check the CORS choice in their options.

## Limits and host responsibilities

The decoded body is bounded at 1 MiB by default, which protects the process. A
body within the bound is validated with the byte limit raised to the bound when
it exceeds the default 32 MiB, with the default 1,000,000 nodes and 10,000
container levels. Past either cap, the result is undetermined with `node-limit`
or `depth-limit`; validate the retained body with
`openbindings::validate_document_with_limits` and a larger `max_nodes` or
`max_depth`, respectively. An
attempt has no deadline unless `ClientOptions::timeout` sets one; without one it
runs until it completes or the caller cancels it. A deadline only bounds time, so
it is the application's choice: a host that needs a bound sets it. Only 404 means
absence; 401 and 403 mean gated discovery. HTTP status, transport failure,
cancellation, timeout and document validation remain distinct. Complete bounded
response bytes and observed metadata survive validation refusal.

Native and portable discovery require HTTPS except literal IPv4 127/8 or IPv6
::1 by default. Set `ClientOptions::allow_http` to `true` to admit named or other
plain HTTP origins and HTTP-to-HTTP redirects. Plain HTTP exposes requests,
credentials and documents to network observation and modification. Every
HTTPS-to-HTTP redirect is still refused; credential and other admission rules
are unchanged. `endpoint(origin)` keeps the default policy; `options.endpoint(origin)`
uses the selected policy. A refused plain-HTTP origin keeps code `origin-policy`.

Native discovery checks redirect policy before each hop and runs on a Tokio
runtime with timers enabled. It keeps
reqwest's redirect execution, which recovers a Location that is not an RFC 3986
URI-reference, as RFC 9110 section 10.2.2 permits: a space is percent-encoded and
a backslash is read as a slash. Portable Rust rejects non-URI-references;
TypeScript rejects whitespace, non-ASCII characters and backslashes, but its
WHATWG resolution can admit other malformed references such as `/document%ZZ`.
Every path admits the URL each hop actually requests. Applications
own TLS roots, proxies, network-address restrictions and credentials, including
which origins receive them across redirects. Portable transports must honor the
supplied URL, decoded-byte, redirect and cancellation contracts. Browser Fetch
cannot expose all hops or peer addresses. TypeScript uses `allowHttp: true`;
when enabled, a hidden redirect is a `redirect-policy` transport error without
automatic following. Browser and Worker host restrictions still apply, including
mixed-content blocking; a rejected Fetch request is a network transport error.

Publication defaults to `Access-Control-Allow-Origin: *`. Authentication,
credentialed CORS and preflight belong to application middleware. Synchronous
validation cannot be preempted by a timer; hard CPU deadlines require isolation.

Read the [transport and publication policy](https://github.com/openbindings/sdk/blob/main/docs/http-discovery.md)
before configuring custom transport or delegated policy. This unpublished
package implements HTTP Discovery 0.1.0 and requires Rust 1.88.

## Reference

See `Publication`, `PublicationOptions`, `DiscoveryOutcome`, `ClientOptions`
and the `native` module. TypeScript callers use
`@openbindings/sdk/http-discovery` as described in the
[TypeScript guide](https://github.com/openbindings/sdk/blob/main/packages/typescript/README.md).
