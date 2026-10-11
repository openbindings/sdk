#![allow(dead_code)]
use openbindings::{DocumentBuilder, JsonRef, JsonValue, ParsedDocument, Source, WorkControl};
use openbindings_interface_synthesis::*;
use std::{
    collections::BTreeMap,
    future::{Ready, ready},
    sync::atomic::{AtomicUsize, Ordering},
};
pub const KIND: &str = "example.coverage@1";
pub fn json(s: &str) -> JsonValue {
    JsonValue::parse(s).unwrap()
}
pub fn source(s: Option<&str>) -> Source {
    Source {
        kind: KIND.into(),
        content: s.map(json),
        ..Default::default()
    }
}
pub fn candidate(s: &Source) -> SynthesisCandidate {
    let mut b = DocumentBuilder::new();
    b.sources = Some(BTreeMap::from([("api".into(), s.clone())]));
    SynthesisCandidate {
        document: b.build().unwrap(),
        coverage: CoverageDraft {
            exhaustive: true,
            ..Default::default()
        },
    }
}
pub struct Fixed {
    pub output: Result<SynthesisCandidate, SynthesisError>,
    pub calls: AtomicUsize,
}
impl Fixed {
    pub fn new(candidate: SynthesisCandidate) -> Self {
        Self {
            output: Ok(candidate),
            calls: AtomicUsize::new(0),
        }
    }
}
impl InterfaceSynthesizer for Fixed {
    type Synthesis<'a> = Ready<Result<SynthesisCandidate, SynthesisError>>;
    fn supports_kind(&self, k: &str) -> bool {
        k == KIND
    }
    fn synthesize<'a>(
        &'a self,
        _: &'a SynthesisRequest<'a>,
        _: WorkControl,
    ) -> Self::Synthesis<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        ready(self.output.clone())
    }
}
pub async fn deliver(
    source: &Source,
    candidate: SynthesisCandidate,
) -> Result<Synthesis, SynthesisError> {
    let inputs = [SynthesisSource {
        name: Some("api"),
        source,
    }];
    let r = SynthesisRequest {
        sources: &inputs,
        ..Default::default()
    };
    Synthesizer::new(Fixed::new(candidate))
        .synthesize_with_coverage(&r)
        .await
}
pub fn optional(v: JsonRef<'_>, key: &str) -> Option<String> {
    v.get(key).map(|v| v.as_str().unwrap().to_owned())
}
pub fn fields(v: Option<JsonRef<'_>>) -> Option<BTreeMap<String, JsonValue>> {
    v.map(|v| {
        v.members()
            .unwrap()
            .map(|m| (m.name.as_str().unwrap().into(), json(m.value.text())))
            .collect()
    })
}
pub fn entry(v: JsonRef<'_>) -> CoverageEntry {
    let mut e = CoverageEntry::new(
        v.get("sourceIndex")
            .unwrap()
            .number_text()
            .unwrap()
            .parse()
            .unwrap(),
        v.get("sourceRef").unwrap().as_str().unwrap(),
        match v.get("scope").unwrap().as_str().unwrap() {
            "target" => CoverageScope::Target,
            "alternative" => CoverageScope::Alternative,
            "projection" => CoverageScope::Projection,
            "dependency" => CoverageScope::Dependency,
            _ => panic!(),
        },
        match v.get("status").unwrap().as_str().unwrap() {
            "represented" => CoverageStatus::Represented,
            "invalid" => CoverageStatus::Invalid,
            "excluded" => CoverageStatus::Excluded,
            "lossy" => CoverageStatus::Lossy,
            "implementation-unsupported" => CoverageStatus::ImplementationUnsupported,
            _ => panic!(),
        },
    );
    e.source_key = optional(v, "sourceKey");
    e.operation_key = optional(v, "operationKey");
    e.binding_key = optional(v, "bindingKey");
    e.dependency_key = optional(v, "dependencyKey");
    e.reason_code = optional(v, "reasonCode");
    e.rule = optional(v, "rule");
    e.message = optional(v, "message");
    e.details = fields(v.get("details"));
    e.requirements = v.get("requirements").map(|a| {
        a.elements()
            .unwrap()
            .map(|x| x.as_str().unwrap().into())
            .collect()
    });
    e
}
pub fn from_case(v: JsonRef<'_>) -> SynthesisCandidate {
    SynthesisCandidate {
        document: ParsedDocument::parse(v.get("interface").unwrap().text()).unwrap(),
        coverage: CoverageDraft {
            entries: v
                .get("entries")
                .unwrap()
                .elements()
                .unwrap()
                .map(entry)
                .collect(),
            exhaustive: v.get("exhaustive").unwrap().as_bool().unwrap(),
            limitation: v.get("limitation").map(|l| CoverageLimitation {
                code: optional(l, "code").unwrap(),
                message: optional(l, "message").unwrap(),
                details: fields(l.get("details")),
                additional_fields: BTreeMap::new(),
            }),
            ..Default::default()
        },
    }
}
pub fn corpus() -> JsonValue {
    json(include_str!("../fixtures/coverage-cases.json"))
}
pub fn complete() -> (Source, SynthesisCandidate) {
    let c = corpus();
    let first = c.get("tests").unwrap().elements().unwrap().next().unwrap();
    let candidate = from_case(first);
    let source = candidate
        .document
        .to_authoring()
        .unwrap()
        .sources
        .unwrap()
        .remove("api")
        .unwrap();
    (source, candidate)
}
