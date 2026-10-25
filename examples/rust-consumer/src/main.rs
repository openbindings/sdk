//! Six ordinary SDK workflows; no internal crate, ABI, CLI or invoker dependency.
use openbindings::*;
use openbindings_http_discovery::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    time::Duration,
};
fn exact(text: &str) -> JsonValue {
    JsonValue::parse(text).expect("example fixture")
}
fn author() -> Result<(Document, Document), Box<dyn std::error::Error>> {
    let mut editable = Document::new();
    editable.set_operation(
        "run",
        &Operation {
            aliases: Some(vec!["execute".into()]),
            input: Some(exact(r#"{"$ref":"https://example.invalid/integer"}"#)),
            output: Some(exact("false")),
            examples: Some(BTreeMap::from([(
                "demo".into(),
                Example {
                    input: Some(exact("9007199254740993")),
                    output: Some(JsonValue::null()),
                    ..Default::default()
                },
            )])),
            ..Default::default()
        },
    )?;
    editable.set_operation("no_contract", &Operation::default())?;
    editable.set_sources(&BTreeMap::from([(
        "local".into(),
        Source {
            kind: "https://example.invalid/raw".into(),
            content: Some(JsonValue::null()),
            ..Default::default()
        },
    )]))?;
    editable.set_bindings(&BTreeMap::from([(
        "binding".into(),
        Binding {
            operation: "run".into(),
            source: "local".into(),
            preference: Preference::new(1),
            content: Some(exact("9007199254740993")),
            ..Default::default()
        },
    )]))?;
    editable.set_dependencies(&BTreeMap::from([(
        "dep".into(),
        Dependency {
            operation: "run".into(),
            kinds: Some(vec!["https://example.invalid/raw".into()]),
            ..Default::default()
        },
    )]))?;
    editable.set("/schemas", &exact(r#"{"boolean":true}"#))?;
    editable.set("/x-application", &exact(r#"{"data":9007199254740993}"#))?;
    let document = editable.clone();
    assert!(document.validate()?.conformant().is_some());
    let serialized = editable.as_bytes();
    let round = Document::parse(serialized)?;
    assert_eq!(document.value().semantic_eq(round.value()), Some(true));
    Ok((editable, document))
}
fn inspect() -> Result<(), Box<dyn std::error::Error>> {
    let broken = validate_document("{broken")?;
    assert!(broken.conformant().is_none());
    assert_eq!(broken.report().evidence["OBI-01"], Evidence::Violated);
    let bad = validate_document(r#"{"openbindings":"0.2.0","operations":{},"unknown":true}"#)?;
    assert_eq!(bad.report().conclusion, Conformance::NonConformant);
    assert!(bad.conformant().is_none());
    for finding in &bad.report().findings {
        println!("{}", serde_json::to_string(finding)?);
    }
    assert!(validate_document(r#"{"openbindings":"8.0.0","operations":{}}"#).is_err());
    Ok(())
}
fn value_contracts(document: &Document) -> Result<(), Box<dyn std::error::Error>> {
    let resources = ResourceSet::new([SchemaResource {
        uri: "https://example.invalid/integer".into(),
        document: exact(r#"{"type":"integer","minimum":0}"#),
    }])?;
    let proof = document
        .validate()?
        .conformant()
        .ok_or("document is not conformant")?;
    let evaluator = DefaultEvaluator::new();
    let operation = proof.resolve_operation("run").ok_or("run is missing")?;
    let input = operation
        .input()
        .ok_or("run has no input contract")?
        .prepare_with(&evaluator, &resources)?;
    let output = operation
        .output()
        .ok_or("run has no output contract")?
        .prepare_with(&evaluator, &resources)?;
    for text in ["7", "9007199254740993"] {
        assert!(matches!(
            input.validate(&exact(text)),
            ValueOutcome::Satisfies
        ));
    }
    assert!(matches!(
        input.validate(&exact("-1")),
        ValueOutcome::Fails { .. }
    ));
    assert!(matches!(
        output.validate(&JsonValue::null()),
        ValueOutcome::Fails { .. }
    ));
    let no_contract = proof
        .resolve_operation("no_contract")
        .ok_or("no_contract is missing")?;
    assert!(no_contract.input().is_none());
    // Preparation may retain a resource-dependent schema; either way no value
    // verdict is available without the missing resource.
    let unresolved = match operation
        .input()
        .ok_or("run has no input contract")?
        .prepare(&evaluator)
    {
        Ok(contract) => contract.validate(&exact("7")),
        Err(detail) => ValueOutcome::NoVerdict { detail },
    };
    assert!(matches!(
        unresolved,
        ValueOutcome::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::ResourceUnavailable,
                ..
            }
        }
    ));
    // Prepared value contracts own what they need after the document and resources go.
    drop((operation, proof, resources));
    assert!(matches!(
        input.validate(&exact("8")),
        ValueOutcome::Satisfies
    ));
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut editable, document) = author()?;
    inspect()?;
    let OperationSelection::Found(operation) = document.resolve_operation("execute")? else {
        return Err("execute did not resolve".into());
    };
    assert_eq!(operation.key(), "run");
    assert_eq!(document.operation_bindings("run")?, vec!["binding"]);
    assert_eq!(
        document.dependency_accepts_kind("dep", "https://example.invalid/raw")?,
        Some(true)
    );
    let references = document.references()?;
    println!("references: {}", serde_json::to_string(&references)?);
    value_contracts(&document)?;
    let valid = document
        .validate()?
        .conformant()
        .expect("conformance established");
    let original = valid.as_bytes().to_vec();
    let publication = Publication::new(&valid, PublicationOptions::default())?;
    editable.set_operations(&BTreeMap::new())?;
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
    let http = reqwest::Client::builder().timeout(Duration::from_secs(5));
    let client = native::Client::new(http, ClientOptions::default())?;
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
        reqwest::Client::builder(),
        ClientOptions {
            max_document_bytes: 1,
            ..ClientOptions::default()
        },
    )?;
    let result = limited.discover(&origin).await?;
    assert!(matches!(result.outcome, DiscoveryOutcome::BodyLimit { .. }));
    assert_eq!(result.metadata.unwrap().status, 200);
    server.join().expect("example server")?;
    // The default evaluator's qualified limitations. It decides the four cases
    // of those capabilities whose verdicts do not depend on a property escape's
    // match.
    let mut options = openbindings_schema_evaluator_test_support::Options::without(&[
        "unicode-property-matching",
        "backtracking-pattern-matching",
        "undefined-results",
    ]);
    options.permitted_refusals.retain(|id, _| {
        !id.starts_with("suite/draft2020-12/patternProperties.json#5/")
            && !matches!(
                id.as_str(),
                "adversarial/lazy-pattern-properties/0" | "adversarial/lazy-pattern-properties/1"
            )
    });
    let report = openbindings_schema_evaluator_test_support::run(&DefaultEvaluator::new(), &options);
    assert!(report.is_success());
    println!(
        "six public workflows and {} evaluator cases passed",
        report.observations.len()
    );
    Ok(())
}
