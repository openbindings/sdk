# OpenBindings HTTP discovery

This optional companion applies HTTP Discovery 0.1.0 at revision
`2f7d754dc2da374058cd517064c17e50f7d95d99`. Its client and publication
capabilities are independent of core document conformance.

Enable `native` to use the reqwest adapter:

```rust,no_run
use openbindings_http_discovery::{native::Client, ClientOptions, DiscoveryOutcome};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()?;
let client = Client::new(http, ClientOptions::default())?;
let result = client.discover("https://service.example").await?;
match result.outcome {
    DiscoveryOutcome::Found { document } => {
        let operation = document.parsed().resolve_operation("lookup")?;
    }
    DiscoveryOutcome::Absent => { /* only an observed 404 */ }
    DiscoveryOutcome::Gated => { /* inspect retained authentication headers */ }
    other => { /* handle status, document, capability, body or transport outcome */ }
}
# Ok(())
# }
```

The supplied HTTP client owns TLS, redirect, credential, proxy, network-range,
and timeout policy. The SDK does not mutate it. A `WorkControl` can cancel an
in-flight request or body read; core assessment is synchronous, with cancellation
checked before and after it. `ClientOptions::discover_with` accepts a local
asynchronous transport without requiring `Send` futures.

Origins must use HTTP(S) and contain no credentials, resource path, query, or
fragment. One trailing slash is accepted. Discovery sends the companion's Accept
header and inspects all 200 bodies regardless of media type. The default decoded
body limit is 1 MiB; zero selects that default and negative limits are errors.
Application reads stop at limit plus one byte. A host transport may buffer larger
chunks internally. Oversized prefixes are never assessed as documents.

Once a response is available, metadata survives body failures and cancellation.
Complete bounded 200 bytes also survive document violations, undecided conformance,
and unsupported versions. Only 404 is absence; 401/403 are gated. Other statuses
remain explicit. A native short non-200 HTTP/1 body is discarded within a 2 KiB /
100 ms budget to permit connection reuse. Cleanup cannot replace its status or
cancel the caller's control. Dropping other response bodies closes their state.

Publication requires established conformance and copies exact bytes once:

```rust
use openbindings_http_discovery::{Publication, PublicationOptions, WELL_KNOWN_PATH};
let source = br#"{"openbindings":"0.2.0","operations":{"lookup":{}}}"#;
let publication = Publication::from_bytes(source, PublicationOptions {
    allow_origin: "*".into(),
})?;
let response = publication.respond("GET", WELL_KNOWN_PATH);
assert_eq!(response.status, 200);
assert_eq!(&*response.body, source);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Pass the host's decoded route path, excluding query and fragment. GET serves the
snapshot; HEAD has the same headers and an empty body. Other methods receive 405
with `Allow: GET, HEAD`; other paths receive 404. Accept never hides the default
OBI representation. No listener, authentication middleware, dependency wiring,
source acquisition, or schema retrieval is created.

The fixed CORS value may be empty, `*`, `null`, or one ASCII origin. It is checked
and emitted unchanged. Credentialed CORS and preflight remain application policy.
The TypeScript package provides the corresponding Fetch client and
`DiscoveryPublication.respond(request)` helper. Browsers may hide headers and
redirect destinations; those unavailable facts are not invented by the facade.

Version 0.2.0-alpha.1 is an unpublished candidate requiring Rust 1.99. Package publication and application cutover are separate actions.

Definition-level reference contracts, rendered documentation and maintained checks
are described in the [API reference guide](../../docs/api-reference.md).
