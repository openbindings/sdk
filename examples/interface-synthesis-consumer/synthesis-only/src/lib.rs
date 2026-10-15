//! Compiles without the inspection companion anywhere in its dependency graph.
use openbindings::Document;
use openbindings_interface_synthesis::CancellationToken;
use openbindings_interface_synthesis::*;
use std::future::{Ready, ready};
pub struct Skeleton;
impl InterfaceSynthesizer for Skeleton {
    type Synthesis<'a> = Ready<Result<SynthesisCandidate, SynthesisError>>;
    fn supports_kind(&self, _: &str) -> bool {
        false
    }
    fn synthesize<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
        _: CancellationToken,
    ) -> Self::Synthesis<'a> {
        ready((|| {
            let mut document = Document::new();
            document.set_openbindings(
                r.openbindings_version
                    .unwrap_or(openbindings::AUTHORING_VERSION),
            )?;
            if let Some(value) = r.name {
                document.set_name(value)?;
            }
            if let Some(value) = r.version {
                document.set_version(value)?;
            }
            if let Some(value) = r.description {
                document.set_description(value)?;
            }
            Ok(SynthesisCandidate {
                document,
                coverage: CoverageDraft {
                    exhaustive: true,
                    ..Default::default()
                },
            })
        })())
    }
}
#[cfg(test)]
#[tokio::test]
async fn source_free_overrides_and_ownership() {
    let provider = Synthesizer::new(Skeleton);
    let request = SynthesisRequest {
        openbindings_version: Some("0.2.7+test"),
        name: Some("Empty API"),
        version: Some("app.v1"),
        description: Some("Metadata is still meaningful"),
        ..Default::default()
    };
    let result = provider.synthesize_with_coverage(&request).await.unwrap();
    let strict = provider.synthesize(&request).await.unwrap();
    assert_eq!(
        result.document().as_document().to_bytes(),
        strict.to_bytes()
    );
    assert_eq!(
        strict
            .as_document()
            .value()
            .get("version")
            .unwrap()
            .as_str(),
        Some("app.v1")
    );
    assert_eq!(
        strict
            .as_document()
            .value()
            .get("description")
            .unwrap()
            .as_str(),
        request.description
    );
    assert_eq!(result.coverage().entries().len(), 0);
    assert!(result.coverage().fully_represented());
    drop(provider);
    assert!(result.to_json().unwrap().text().contains("Empty API"));
}
