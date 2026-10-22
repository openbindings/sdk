//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::{Document, DocumentValidation, VersionRefusal};
use openbindings_http_discovery as http;
use std::fmt::Debug;
use std::sync::Arc;
// A credential-shaped probe value; Debug output must never contain it.
const SECRET: &str = "PROBE-BEARER-TOKEN-7391";
const NUMBER: &str = "739173917391";

struct Row {
    label: &'static str,
    text: [String; 4],
}
fn row(label: &'static str, value: &(impl Debug + ?Sized)) -> Row {
    Row {
        label,
        text: [
            format!("{value:?}"),
            format!("{value:#?}"),
            format!("{:?}", Some(value)),
            format!("{:#?}", Some(value)),
        ],
    }
}
struct Clock;
impl http::DiscoveryTimer for Clock {
    fn now(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
    async fn sleep_until(&self, _: std::time::Duration) {
        std::future::pending::<()>().await;
    }
}
struct Empty;
impl http::ResponseBody for Empty {
    async fn read(&mut self, _: &mut [u8]) -> Result<usize, http::RequestError> {
        Ok(0)
    }
}
/// One discovery an in-memory transport answers with 404: the request the
/// client sent and its receipt.
async fn absent() -> (http::DiscoveryRequest, http::DiscoveryResult) {
    let mut sent = None;
    let result = http::discover(
        "https://example.invalid",
        &http::ClientOptions::default(),
        &http::CancellationToken::new(),
        &Clock,
        |request| {
            sent = Some(request);
            async {
                Ok::<_, http::RequestError>(http::HttpResponse {
                    final_url: None,
                    status: 404,
                    headers: Vec::new(),
                    body: Empty,
                })
            }
        },
    )
    .await
    .unwrap();
    (sent.expect("one request"), result)
}
// Discovery and publication build these records; a caller receives them and
// can reassign their public fields, so these probes fill actual records with
// caller text.
async fn http_rows(
    reference: &str,
    version: &VersionRefusal,
    validation: &DocumentValidation,
) -> Vec<Row> {
    let exact = http::AllowOrigin::Exact(format!("https://user:{SECRET}@example.invalid"));
    let mut config = http::Publication::new(
        &validation.conformant().unwrap(),
        http::PublicationOptions {
            allow_origin: exact.clone(),
        },
    )
    .unwrap_err();
    let mut rows = Vec::new();
    for (variant, policy) in [
        ("Exact", exact),
        ("Any", http::AllowOrigin::Any),
        ("Omit", http::AllowOrigin::Omit),
        ("Null", http::AllowOrigin::Null),
    ] {
        let item = row("AllowOrigin", &policy);
        assert!(item.text.iter().all(|text| text.contains(variant)));
        rows.push(item);
        rows.push(row(
            "PublicationOptions",
            &http::PublicationOptions {
                allow_origin: policy,
            },
        ));
    }
    let header = http::Header::new(SECRET, SECRET);
    let (mut request, mut result) = absent().await;
    let mut metadata = result.metadata.clone().expect("a received response");
    metadata.requested_url = reference.into();
    metadata.final_url = Some(reference.into());
    metadata.status = 200;
    metadata.headers = vec![header.clone()];
    let error = http::RequestError::new(http::ErrorKind::Network, SECRET);
    assert_eq!(error.to_string(), SECRET);
    config.code = SECRET;
    config.message = SECRET;
    assert_eq!(config.to_string(), SECRET);
    let body: Arc<[u8]> = Arc::from(SECRET.as_bytes());
    result.outcome = http::DiscoveryOutcome::TransportError {
        error: error.clone(),
    };
    result.metadata = Some(metadata.clone());
    result.body = Some(body.clone());
    request.url = reference.into();
    request.accept = SECRET;
    let mut response = http::Publication::new(
        &validation.conformant().unwrap(),
        http::PublicationOptions::default(),
    )
    .unwrap()
    .respond("GET", http::WELL_KNOWN_PATH);
    response.status = 200;
    response.headers = vec![header.clone()];
    response.body = body;
    rows.extend([
        row("Header", &header),
        row("ResponseMetadata", &metadata),
        row("RequestError", &error),
        row("ConfigurationError", &config),
        row(
            "PublicationError::Configuration",
            &http::PublicationError::Configuration(config),
        ),
        row(
            "PublicationError::VersionRefused",
            &http::PublicationError::VersionRefused(version.clone()),
        ),
        row(
            "PublicationError::NonConformant",
            &http::PublicationError::NonConformant(validation.clone()),
        ),
        row("DiscoveryResult", &result),
        row("DiscoveryRequest", &request),
        row(
            "HttpResponse",
            &http::HttpResponse {
                final_url: Some(reference.into()),
                status: 200,
                headers: vec![header.clone()],
                body: SECRET,
            },
        ),
        row("PublicationResponse", &response),
    ]);
    let publication = http::Publication::new(
        &validation.conformant().unwrap(),
        http::PublicationOptions {
            allow_origin: http::AllowOrigin::Exact(format!(
                "https://{}.invalid",
                SECRET.to_lowercase()
            )),
        },
    )
    .unwrap();
    rows.push(row("Publication", &publication));
    rows
}

fn check_rows(rows: Vec<Row>, expected: usize) {
    assert_eq!(rows.len(), expected);
    for Row { label, text } in &rows {
        for (mode, text) in ["ordinary", "pretty", "nested", "nested pretty"]
            .into_iter()
            .zip(text)
        {
            for marker in [SECRET, NUMBER] {
                assert!(!text.contains(marker), "{label}/{mode} leaked: {text}");
            }
        }
    }
    println!("{} redaction rows, four renderings each", rows.len());
}

#[tokio::test(flavor = "current_thread")]
async fn debug_redacts_caller_text_at_every_boundary() {
    let reference = format!("https://user:{SECRET}@example.invalid/schema?access_token={SECRET}");
    let parsed = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"properties":{"MEMBER":{"type":"string"}}}}}}"#.replace("MEMBER", SECRET)).unwrap();
    let validation = parsed.validate().unwrap();
    let mut version = VersionRefusal::new("0.3.0").unwrap();
    version.declared = SECRET.into();
    version.supported = SECRET;
    let rows = http_rows(&reference, &version, &validation).await;

    check_rows(rows, 20);
}
