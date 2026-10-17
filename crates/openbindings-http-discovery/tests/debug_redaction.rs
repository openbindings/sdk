use openbindings_http_discovery::*;
use std::sync::Arc;

const SECRET: &str = "PRIVATE-CREDENTIAL-SOURCE";
fn redacted(value: &dyn std::fmt::Debug) {
    for text in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(!text.contains(SECRET), "{text}");
    }
}
struct Clock;
impl DiscoveryTimer for Clock {
    fn now(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
    async fn sleep_until(&self, _: std::time::Duration) {
        std::future::pending::<()>().await;
    }
}
struct Text(Vec<u8>);
impl ResponseBody for Text {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize, RequestError> {
        let n = output.len().min(self.0.len());
        output[..n].copy_from_slice(&self.0[..n]);
        self.0.drain(..n);
        Ok(n)
    }
}
#[tokio::test(flavor = "current_thread")]
async fn http_debug_omits_credentials_urls_and_source_bodies() {
    let header = Header::new("authorization", SECRET);
    redacted(&header);
    // Discovery builds its receipt from what the transport observed.
    let received = discover(
        "https://example.invalid",
        &ClientOptions::default(),
        &CancellationToken::new(),
        &Clock,
        |_| {
            let header = header.clone();
            async move {
                Ok::<_, RequestError>(HttpResponse {
                    final_url: None,
                    status: 200,
                    headers: vec![header],
                    body: Text(SECRET.into()),
                })
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(received.body.as_deref(), Some(SECRET.as_bytes()));
    redacted(&received);
    let mut metadata = received.metadata.clone().expect("a received response");
    metadata.requested_url = SECRET.into();
    metadata.final_url = Some(format!("https://example.invalid/?token={SECRET}"));
    redacted(&metadata);
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
    let mut absent = received;
    absent.outcome = DiscoveryOutcome::Absent;
    absent.metadata = None;
    absent.body = Some(Arc::from(SECRET.as_bytes()));
    redacted(&absent);
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
