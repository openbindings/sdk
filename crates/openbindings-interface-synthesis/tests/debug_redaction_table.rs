//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use openbindings_interface_synthesis as synthesis;
use std::fmt::Debug;
use std::{
    collections::BTreeMap,
    future::{Ready, ready},
};
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

async fn synthesis_rows(source: &Source, parsed: &Document) -> Vec<Row> {
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
    let mut entry = synthesis::CoverageEntry::new(
        0,
        SECRET,
        synthesis::CoverageScope::Target,
        synthesis::CoverageStatus::Represented,
    );
    entry.source_key = Some(SECRET.into());
    entry.operation_key = Some(SECRET.into());
    entry.binding_key = Some(SECRET.into());
    entry.dependency_key = Some(SECRET.into());
    entry.reason_code = Some(SECRET.into());
    entry.rule = Some(SECRET.into());
    entry.message = Some(SECRET.into());
    entry.requirements = Some(vec![SECRET.into()]);
    entry.details = Some(evidence());
    entry.additional_fields = evidence();
    let limitation = synthesis::CoverageLimitation::new(SECRET, SECRET)
        .with_details(evidence())
        .with_additional_fields(evidence());
    let mut coverage = synthesis::CoverageClaim::partial(vec![entry.clone()], limitation.clone());
    coverage.additional_fields = evidence();
    let candidate = synthesis::SynthesisCandidate::new(parsed.clone(), coverage.clone());
    let plain = synthesis::SynthesisRequest {
        sources: &sources,
        ..Default::default()
    };
    let rejected = synthesis::Synthesizer::new(Provider(Ok(candidate.clone())))
        .synthesize_with_coverage(&plain)
        .await
        .unwrap_err();
    // The facade builds rejections and findings; a caller receives them and can
    // reassign their public fields, so these probes fill actual records with
    // caller text.
    let mut rejection = rejected.rejected().expect("a rejected candidate").clone();
    let mut finding = rejection.finding.clone().expect("a correspondence finding");
    finding.code = SECRET.into();
    finding.path = format!("/{SECRET}");
    finding.message = SECRET.into();
    rejection.candidate = candidate.clone();
    rejection.validation = Some(parsed.validate().unwrap());
    rejection.finding = Some(finding.clone());
    let mut rows = vec![
        row("SynthesisSource", &sources[0]),
        row("SynthesisRequest", &request),
        row("SynthesisError::provider", &error),
        row(
            "synthesis SupportedKind",
            &synthesis::SupportedKind::new(SECRET).with_description(SECRET),
        ),
        row(
            "synthesis KindSupportVerdict",
            &synthesis::Synthesizer::new(Provider(Ok(candidate.clone())))
                .check_kind_support([SECRET])
                .unwrap()[0],
        ),
        row("CoverageEntry", &entry),
        row("CoverageLimitation", &limitation),
        row("CoverageClaim", &coverage),
        row("SynthesisCandidate", &candidate),
        row("SynthesisFinding", &finding),
        row("RejectedSynthesis", &rejection),
    ];
    // A secret-named key with a nonsecret null value.
    let mut key_only = entry;
    key_only.details = Some(BTreeMap::from([(
        format!("credential-{SECRET}"),
        JsonValue::null(),
    )]));
    rows.push(row("CoverageEntry key only", &key_only));
    assert!(rejected.rejected().is_some());
    rows.push(row("SynthesisError::rejected", &rejected));
    let mut document = Document::new();
    document.set_source(SECRET, source).unwrap();
    let mut coverage = synthesis::CoverageClaim::partial(Vec::new(), limitation);
    coverage.additional_fields = evidence();
    let candidate = synthesis::SynthesisCandidate::new(document.clone(), coverage);
    let facade = synthesis::Synthesizer::new(Provider(Ok(candidate)));
    let checked = facade.synthesize_with_coverage(&plain).await.unwrap();
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
    let incomplete = facade.synthesize(&plain).await.unwrap_err();
    assert!(incomplete.partial().is_some());
    rows.push(row("SynthesisError::partial", &incomplete));
    let provider =
        synthesis::SynthesisError::new(synthesis::SynthesisErrorKind::Incomplete, SECRET, SECRET);
    let wrapped = synthesis::Synthesizer::new(Provider(Err(provider)))
        .synthesize(&plain)
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
    let parsed = Document::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"properties":{SECRET:{"type":"string"}}}}}}).to_string()).unwrap();
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
    let collision = draft.set_operation(SECRET, &operation).unwrap_err();
    let EditError::FieldCollision { part_pointer, .. } = &collision else {
        panic!()
    };
    assert!(part_pointer.as_deref().unwrap().contains(SECRET));
    let mut rows = synthesis_rows(&source, &parsed).await;
    rows.push(row(
        "SynthesisError::authoring",
        &synthesis::SynthesisError::from(collision),
    ));

    check_rows(rows, 19);
}
