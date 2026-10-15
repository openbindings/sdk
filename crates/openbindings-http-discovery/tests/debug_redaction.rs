use openbindings_http_discovery::*;
use std::sync::Arc;

const SECRET: &str = "PRIVATE-CREDENTIAL-SOURCE";
fn redacted(value: &dyn std::fmt::Debug) {
    for text in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(!text.contains(SECRET), "{text}");
    }
}
#[test]
fn http_debug_omits_credentials_urls_and_source_bodies() {
    let header = Header::new("authorization", SECRET);
    redacted(&header);
    redacted(&ResponseMetadata {
        requested_url: SECRET.into(),
        status: 200,
        final_url: Some(format!("https://example.invalid/?token={SECRET}")),
        headers: vec![header.clone()],
    });
    // A transport's body need not implement Debug.
    struct Body;
    redacted(&HttpResponse {
        status: 200,
        final_url: Some(SECRET.into()),
        headers: vec![header],
        body: Body,
    });
    let bytes = format!(r#"{{"openbindings":"0.2.0","operations":{{}},"x-private":"{SECRET}"}}"#);
    let publication = Publication::from_bytes(bytes, PublicationOptions::default()).unwrap();
    redacted(&publication);
    redacted(&publication.respond("GET", WELL_KNOWN_PATH));
    redacted(&DiscoveryResult {
        outcome: DiscoveryOutcome::Absent,
        metadata: None,
        body: Some(Arc::from(SECRET.as_bytes())),
    });
}
#[cfg(feature = "native")]
#[test]
fn native_client_debug_omits_default_credentials() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_static(SECRET),
    );
    let client = native::Client::new(
        reqwest::Client::builder().default_headers(headers),
        ClientOptions::default(),
    )
    .unwrap();
    redacted(&client);
}
