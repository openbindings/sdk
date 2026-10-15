//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use openbindings_interface_synthesis as synthesis;
use std::fmt::Debug;
use std::{
    collections::BTreeMap,
    future::{Ready, ready},
};
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
fn evidence() -> BTreeMap<String, JsonValue> {
    BTreeMap::from([(
        format!("credential-{SECRET}"),
        JsonValue::string(SECRET).unwrap(),
    )])
}
struct Provider(Result<synthesis::SynthesisCandidate, synthesis::SynthesisError>);
impl synthesis::InterfaceSynthesizer for Provider {
    type Synthesis<'a> = Ready<Result<synthesis::SynthesisCandidate, synthesis::SynthesisError>>;
    fn supports_kind(&self, _: &str) -> bool {
        true
    }
    fn synthesize<'a>(
        &'a self,
        _: &'a synthesis::SynthesisRequest<'a>,
        _: synthesis::CancellationToken,
    ) -> Self::Synthesis<'a> {
        ready(self.0.clone())
    }
}

async fn synthesis_rows(source: &Source, parsed: &ParsedDocument) -> Vec<Row> {
    let sources = [synthesis::SynthesisSource {
        name: Some(SECRET),
        source,
    }];
    let request = synthesis::SynthesisRequest {
        sources: &sources,
        openbindings_version: Some(SECRET),
        name: Some(SECRET),
        version: Some(SECRET),
        description: Some(SECRET),
    };
    let error = synthesis::SynthesisError::new(
        synthesis::SynthesisErrorKind::ProviderFailure,
        SECRET,
        format!("provider refused schema operand {SECRET}"),
    );
    assert_eq!(error.code(), SECRET);
    assert!(error.to_string().contains(SECRET));
    assert!(format!("{error:?}").contains("ProviderFailure"));
    let entry = synthesis::CoverageEntry {
        source_index: 0,
        source_ref: SECRET.into(),
        scope: synthesis::CoverageScope::Target,
        status: synthesis::CoverageStatus::Represented,
        source_key: Some(SECRET.into()),
        operation_key: Some(SECRET.into()),
        binding_key: Some(SECRET.into()),
        dependency_key: Some(SECRET.into()),
        reason_code: Some(SECRET.into()),
        rule: Some(SECRET.into()),
        message: Some(SECRET.into()),
        requirements: Some(vec![SECRET.into()]),
        details: Some(evidence()),
        additional_fields: evidence(),
    };
    let limitation = synthesis::CoverageLimitation {
        code: SECRET.into(),
        message: SECRET.into(),
        details: Some(evidence()),
        additional_fields: evidence(),
    };
    let coverage = synthesis::CoverageDraft {
        entries: vec![entry.clone()],
        exhaustive: false,
        limitation: Some(limitation.clone()),
        additional_fields: evidence(),
    };
    let candidate = synthesis::SynthesisCandidate {
        document: parsed.clone(),
        coverage: coverage.clone(),
    };
    let finding = synthesis::SynthesisFinding {
        code: SECRET.into(),
        path: format!("/{SECRET}"),
        message: SECRET.into(),
    };
    let mut rows = vec![
        row("SynthesisSource", &sources[0]),
        row("SynthesisRequest", &request),
        row("SynthesisError::provider", &error),
        row(
            "synthesis SupportedKind",
            &synthesis::SupportedKind {
                kind: SECRET.into(),
                description: Some(SECRET.into()),
            },
        ),
        row(
            "synthesis KindSupportVerdict",
            &synthesis::KindSupportVerdict {
                kind: SECRET.into(),
                supported: true,
            },
        ),
        row("CoverageEntry", &entry),
        row("CoverageLimitation", &limitation),
        row("CoverageDraft", &coverage),
        row("SynthesisCandidate", &candidate),
        row("SynthesisFinding", &finding),
        row(
            "RejectedSynthesis",
            &synthesis::RejectedSynthesis {
                candidate: candidate.clone(),
                validation: Some(parsed.validate().unwrap()),
                finding: Some(finding),
            },
        ),
    ];
    // The review also used a secret key with a nonsecret null value.
    let mut key_only = entry;
    key_only.details = Some(BTreeMap::from([(
        format!("credential-{SECRET}"),
        JsonValue::null(),
    )]));
    rows.push(row("CoverageEntry key only", &key_only));
    let request = synthesis::SynthesisRequest {
        sources: &sources,
        ..Default::default()
    };
    let rejected = synthesis::Synthesizer::new(Provider(Ok(candidate)))
        .synthesize_with_coverage(&request)
        .await
        .unwrap_err();
    assert!(rejected.rejected().is_some());
    rows.push(row("SynthesisError::rejected", &rejected));
    let mut document = Document::new();
    document.sources = Some(BTreeMap::from([(SECRET.into(), source.clone())]));
    let candidate = synthesis::SynthesisCandidate {
        document: document.build().unwrap(),
        coverage: synthesis::CoverageDraft {
            exhaustive: false,
            limitation: Some(limitation),
            additional_fields: evidence(),
            ..Default::default()
        },
    };
    let facade = synthesis::Synthesizer::new(Provider(Ok(candidate)));
    let checked = facade.synthesize_with_coverage(&request).await.unwrap();
    rows.extend([
        row("Synthesizer", &facade),
        row("Synthesis", &checked),
        row("SynthesisCoverage", checked.coverage()),
    ]);
    assert!(checked.coverage().source_keys()[0].contains(SECRET));
    assert!(
        serde_json::to_string(checked.coverage())
            .unwrap()
            .contains(SECRET)
    );
    let incomplete = facade.synthesize(&request).await.unwrap_err();
    assert!(incomplete.partial().is_some());
    rows.push(row("SynthesisError::partial", &incomplete));
    let provider =
        synthesis::SynthesisError::new(synthesis::SynthesisErrorKind::Incomplete, SECRET, SECRET);
    let wrapped = synthesis::Synthesizer::new(Provider(Err(provider)))
        .synthesize(&request)
        .await
        .unwrap_err();
    assert!(std::error::Error::source(&wrapped).is_some());
    rows.push(row("SynthesisError::nested provider", &wrapped));
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

#[tokio::test]
async fn debug_redacts_caller_text_at_every_boundary() {
    let context = JsonValue::parse(
        serde_json::json!({"authorization":format!("Bearer {SECRET}"), SECRET: SECRET}).to_string(),
    )
    .unwrap();
    let parsed = ParsedDocument::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"properties":{SECRET:{"type":"string"}}}}}}).to_string()).unwrap();
    let source = Source {
        kind: SECRET.into(),
        content: Some(context.clone()),
        description: Some(SECRET.into()),
        additional_fields: BTreeMap::from([(format!("x-{SECRET}"), context.clone())]),
    };
    let mut draft = Document::new();
    let mut operation = Operation::default();
    operation
        .additional_fields
        .insert("description".into(), JsonValue::null());
    draft.operations.insert(SECRET.into(), operation);
    let authoring = draft.build().unwrap_err();
    assert!(authoring.draft_pointer().unwrap().contains(SECRET));
    let mut rows = synthesis_rows(&source, &parsed).await;
    rows.push(row(
        "SynthesisError::authoring",
        &synthesis::SynthesisError::from(authoring),
    ));

    check_rows(rows, 19);
}
