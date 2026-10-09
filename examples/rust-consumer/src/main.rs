//! Six ordinary SDK workflows; no internal crate, ABI, CLI or invoker dependency.
use openbindings::*;
use openbindings_http_discovery::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};
fn exact(text: &str) -> JsonValue {
    JsonValue::parse(text).expect("example fixture")
}
fn author() -> Result<(DocumentBuilder, ParsedDocument), Box<dyn std::error::Error>> {
    let mut draft = DocumentBuilder::new();
    draft.operations.insert(
        "run".into(),
        Operation {
            aliases: Some(vec!["execute".into()]),
            input: Some(exact(r#"{"$ref":"https://example.invalid/integer"}"#)),
            output: Some(exact("false")),
            examples: Some(BTreeMap::from([(
                "demo".into(),
                OperationExample {
                    input: Some(exact("9007199254740993")),
                    output: Some(JsonValue::null()),
                    ..Default::default()
                },
            )])),
            ..Default::default()
        },
    );
    draft
        .operations
        .insert("no_contract".into(), Operation::default());
    draft.sources = Some(BTreeMap::from([(
        "local".into(),
        Source {
            kind: "https://example.invalid/raw".into(),
            content: Some(JsonValue::null()),
            ..Default::default()
        },
    )]));
    draft.bindings = Some(BTreeMap::from([(
        "binding".into(),
        Binding {
            operation: "run".into(),
            source: "local".into(),
            preference: Preference::new(1),
            content: Some(exact("9007199254740993")),
            ..Default::default()
        },
    )]));
    draft.dependencies = Some(BTreeMap::from([(
        "dep".into(),
        Dependency {
            operation: "run".into(),
            kinds: Some(vec!["https://example.invalid/raw".into()]),
            ..Default::default()
        },
    )]));
    draft.schemas = Some(BTreeMap::from([("boolean".into(), exact("true"))]));
    draft.additional_fields.insert(
        "x-application".into(),
        exact(r#"{"data":9007199254740993}"#),
    );
    let document = draft.build()?;
    assert!(document.assess()?.validated().is_some());
    let serialized = serde_json::to_vec(&draft)?;
    let round = ParsedDocument::parse(serialized)?;
    assert_eq!(document.value().semantic_eq(round.value()), Some(true));
    Ok((draft, document))
}
fn inspect() -> Result<(), Box<dyn std::error::Error>> {
    let broken = assess_document("{broken")?;
    assert!(broken.validated().is_none());
    assert_eq!(broken.report().evidence["OBI-01"], Evidence::Violated);
    let bad = assess_document(r#"{"openbindings":"0.2.0","operations":{},"unknown":true}"#)?;
    assert_eq!(bad.report().conclusion, Conformance::NonConformant);
    assert!(bad.validated().is_none());
    for finding in &bad.report().findings {
        println!("{}", serde_json::to_string(finding)?);
    }
    assert!(assess_document(r#"{"openbindings":"8.0.0","operations":{}}"#).is_err());
    Ok(())
}
fn contracts(document: &ParsedDocument) -> Result<(), Box<dyn std::error::Error>> {
    let resources = ResourceSet::new([SchemaResource {
        uri: "https://example.invalid/integer".into(),
        document: exact(r#"{"type":"integer","minimum":0}"#),
    }])?;
    let context = document.value_contracts(Arc::new(DefaultEvaluator::new()), resources)?;
    let ContractPreparation::Ready(input) = context.prepare("run", Side::Input) else { return Err("input contract did not prepare".into()); };
    let ContractPreparation::Ready(output) = context.prepare("run", Side::Output) else { return Err("output contract did not prepare".into()); };
    for text in ["7", "9007199254740993"] {
        assert!(matches!(
            input.validate(&exact(text)),
            ValueOutcome::Satisfies
        ));
    }
    assert!(matches!(
        input.validate(&exact("-1")),
        ValueOutcome::Mismatch { .. }
    ));
    assert!(matches!(
        output.validate(&JsonValue::null()),
        ValueOutcome::Mismatch { .. }
    ));
    assert!(matches!(
        context
            .prepare("no_contract", Side::Input),
        ContractPreparation::NoContract
    ));
    let unavailable =
        document.value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())?;
    assert!(matches!(
        unavailable
            .prepare("run", Side::Input),
        ContractPreparation::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::ResourceUnavailable,
                ..
            }
        }
    ));
    drop(context);
    assert!(matches!(
        input.validate(&exact("8")),
        ValueOutcome::Satisfies
    ));
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut draft, document) = author()?;
    inspect()?;
    let OperationSelection::Found(operation) = document.resolve_operation("execute")? else { return Err("execute did not resolve".into()); };
    assert_eq!(operation.key(), "run");
    assert_eq!(document.operation_bindings("run")?, vec!["binding"]);
    assert_eq!(
        document.dependency_accepts_kind("dep", "https://example.invalid/raw")?,
        Some(true)
    );
    let references = document.references()?;
    println!("references: {}", serde_json::to_string(&references)?);
    contracts(&document)?;
    let valid = document
        .assess()?
        .validated()
        .expect("conformance established");
    let original = valid.original_bytes().to_vec();
    let publication = Publication::new(&valid, PublicationOptions::default())?;
    draft.operations.clear();
    assert_eq!(
        publication.respond("GET", WELL_KNOWN_PATH).body.as_ref(),
        original
    );
    assert!(publication.respond("HEAD", WELL_KNOWN_PATH).body.is_empty());
    assert_eq!(publication.respond("POST", WELL_KNOWN_PATH).status, 405);
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let origin = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        for case in 0..5 {
            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut reader = BufReader::new(stream);
            let mut first = String::new();
            reader.read_line(&mut first)?;
            assert!(first.starts_with("GET /.well-known/openbindings "));
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            assert!(headers.to_ascii_lowercase().contains(ACCEPT));
            let response = publication.respond("GET", WELL_KNOWN_PATH);
            let (status, body) = match case {
                1 => (404, b"absent".to_vec()),
                2 => (401, b"gated".to_vec()),
                3 => (200, br#"{"openbindings":"8.0.0","operations":{}}"#.to_vec()),
                _ => (response.status, response.body.to_vec()),
            };
            write!(
                reader.get_mut(),
                "HTTP/1.1 {status} Test\r\nContent-Type: {MEDIA_TYPE}\r\nContent-Length: {}\r\nWWW-Authenticate: Bearer realm=example\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            reader.get_mut().write_all(&body)?;
        }
        Ok(())
    });
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let client = native::Client::new(http.clone(), ClientOptions::default())?;
    for expected in ["found", "absent", "gated", "version-refused"] {
        let result = client.discover(&origin).await?;
        assert_eq!(
            result.metadata.as_ref().unwrap().requested_url,
            format!("{origin}{WELL_KNOWN_PATH}")
        );
        let actual = match result.outcome {
            DiscoveryOutcome::Found { .. } => "found",
            DiscoveryOutcome::Absent => "absent",
            DiscoveryOutcome::Gated => "gated",
            DiscoveryOutcome::VersionRefused { .. } => "version-refused",
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(actual, expected);
    }
    let limited = native::Client::new(
        http,
        ClientOptions {
            max_document_bytes: 1,
        },
    )?;
    let result = limited.discover(&origin).await?;
    assert!(matches!(result.outcome, DiscoveryOutcome::BodyLimit { .. }));
    assert_eq!(result.metadata.unwrap().status, 200);
    server.join().expect("example server")?;
    let report = openbindings_schema_evaluator_test_support::run(
        Arc::new(DefaultEvaluator::new()),
        &openbindings_schema_evaluator_test_support::Options::without_unicode_property_matching(),
    );
    assert!(report.is_success());
    println!(
        "six public workflows and {} evaluator cases passed",
        report.observations.len()
    );
    Ok(())
}
