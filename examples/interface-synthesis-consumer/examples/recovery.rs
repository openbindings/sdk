use interface_synthesis_consumer::{Catalog, KIND};
use openbindings::{JsonValue, Source};
use openbindings_interface_synthesis::*;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = Source {
        kind: KIND.into(),
        content: Some(JsonValue::parse(
            r#"{"units":[{"id":"known"},{"id":"gap","status":"implementation-unsupported"}]}"#,
        )?),
        ..Default::default()
    };
    let sources = [SynthesisSource {
        name: Some("catalog"),
        source: &source,
    }];
    let request = SynthesisRequest {
        sources: &sources,
        ..Default::default()
    };
    let provider = Catalog::default();
    let facade = Synthesizer::new(&provider);
    match facade.synthesize(&request).await {
        Ok(document) => println!(
            "Complete document: {} bytes",
            document.parsed().value().text().len()
        ),
        Err(e) if e.kind() == SynthesisErrorKind::Incomplete => {
            let partial = e
                .into_partial()
                .expect("strict refusal retains checked result");
            for entry in partial.coverage().entries() {
                println!("{}: {:?}", entry.source_ref, entry.status);
            }
            println!(
                "Retained {} bytes of document and evidence for explicit acceptance.",
                partial.to_json()?.text().len()
            );
        }
        Err(e) => return Err(e.into()),
    }
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    Ok(())
}
