# OpenBindings HTTP discovery

Discover an OpenBindings interface document at an origin, or publish validated
immutable bytes through your HTTP server. Core document semantics remain
independent of acquisition metadata.

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

For native discovery, enable the `native` feature and create `native::Client`
from your reqwest `ClientBuilder` and `ClientOptions`. Call `discover(origin)`
and match `DiscoveryOutcome`. The portable `discover` function instead accepts
a host transport and `DiscoveryTimer`.

## Limits and host responsibilities

The default decoded body limit is 1 MiB and the whole-attempt deadline is thirty
seconds. Only 404 means absence; 401 and 403 mean gated discovery. HTTP status,
transport failure, cancellation, timeout and document validation remain distinct.
Complete bounded response bytes and observed metadata survive validation refusal.

Native discovery requires HTTPS except literal loopback and checks redirect
policy before each hop. Applications own TLS roots, credentials, proxies and
network-address restrictions. Portable transports must honor the supplied URL,
decoded-byte, redirect and cancellation contracts. Browser Fetch cannot expose
all hops or peer addresses.

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
