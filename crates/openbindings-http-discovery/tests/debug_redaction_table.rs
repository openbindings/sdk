//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::{DocumentValidation, ParsedDocument, VersionRefusal};
use openbindings_http_discovery as http;
use std::fmt::Debug;
use std::sync::Arc;
// Keep the independent review's exact credential and pointer probe value.
const SECRET: &str = "REVIEW-BEARER-TOKEN-7391";
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
fn http_rows(
    reference: &str,
    version: &VersionRefusal,
    validation: &DocumentValidation,
) -> Vec<Row> {
    let exact = http::AllowOrigin::Exact(format!("https://user:{SECRET}@example.invalid"));
    assert!(http::validate_allow_origin(&exact).is_err());
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
    let metadata = http::ResponseMetadata {
        requested_url: reference.into(),
        final_url: Some(reference.into()),
        status: 200,
        headers: vec![header.clone()],
    };
    let error = http::RequestError {
        kind: http::ErrorKind::Network,
        message: SECRET.into(),
    };
    assert_eq!(error.to_string(), SECRET);
    let config = http::ConfigurationError {
        code: SECRET,
        message: SECRET,
    };
    assert_eq!(config.to_string(), SECRET);
    let body: Arc<[u8]> = Arc::from(SECRET.as_bytes());
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
        row(
            "DiscoveryResult",
            &http::DiscoveryResult {
                outcome: http::DiscoveryOutcome::TransportError { error },
                metadata: Some(metadata),
                body: Some(body.clone()),
            },
        ),
        row(
            "DiscoveryRequest",
            &http::DiscoveryRequest {
                url: reference.into(),
                accept: SECRET,
            },
        ),
        row(
            "HttpResponse",
            &http::HttpResponse {
                final_url: Some(reference.into()),
                status: 200,
                headers: vec![header.clone()],
                body: SECRET,
            },
        ),
        row(
            "PublicationResponse",
            &http::PublicationResponse {
                status: 200,
                headers: vec![header],
                body,
            },
        ),
    ]);
    let publication = http::Publication::new(
        &validation.validated().unwrap(),
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

#[test]
fn debug_redacts_caller_text_at_every_boundary() {
    let reference = format!("https://user:{SECRET}@example.invalid/schema?access_token={SECRET}");
    let parsed = ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"properties":{"MEMBER":{"type":"string"}}}}}}"#.replace("MEMBER", SECRET)).unwrap();
    let validation = parsed.validate().unwrap();
    let version = VersionRefusal {
        declared: SECRET.into(),
        supported: SECRET,
    };
    let rows = http_rows(&reference, &version, &validation);

    check_rows(rows, 20);
}
