//! Preparation shares the identity index a resource set built once, instead of
//! admitting the set's resources again, and projects exactly as a fresh
//! admission.
use crate::space;
use openbindings::*;
use openbindings_internal_schema::{self as embedding, SuppliedSet};
use std::sync::{Arc, Mutex};

struct Capture(Mutex<Option<SchemaRequest>>);
impl SchemaEvaluator for Capture {
    fn prepare(
        &self,
        request: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        *self.0.lock().unwrap() = Some(request.clone());
        Err(NoVerdict::new(
            NoVerdictReason::EvaluatorFailure,
            "capture",
            "test capture",
        ))
    }
}
fn request(input: &str, resources: &ResourceSet) -> SchemaRequest {
    let proof = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{input}}}}},"schemas":{{"c":{{"$id":"https://reuse.test/contained","type":"string"}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .expect("conformant document");
    let capture = Capture(Mutex::new(None));
    let _ = proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(&capture, resources);
    capture.0.lock().unwrap().take().unwrap()
}
fn set(entries: &[(&str, &str)]) -> ResourceSet {
    ResourceSet::new(entries.iter().map(|(uri, text)| SchemaResource {
        uri: (*uri).into(),
        document: JsonValue::parse(*text).unwrap(),
    }))
    .unwrap()
}
fn text(program: &embedding::Program) -> String {
    let mut text = program.entry_uri.clone();
    for resource in &program.resources {
        text.push_str(&format!("\n{} {}", resource.uri, resource.document.text()));
    }
    for (uri, origin) in program.locations.iter() {
        text.push_str(&format!("\n{uri} {origin:?}"));
    }
    text
}
fn program(space: &embedding::SchemaSpace, entry: usize) -> String {
    let control = std::sync::atomic::AtomicBool::new(false);
    match space.program(entry, &control) {
        Ok(program) => text(&program),
        Err(failure) => serde_json::to_string(&failure).unwrap(),
    }
}
#[test]
fn preparation_shares_the_sets_admitted_resources() {
    let resources = set(&[
        ("https://reuse.test/a", r#"{"type":"integer"}"#),
        ("https://reuse.test/b", r#"{"$ref":"https://reuse.test/a"}"#),
    ]);
    let request = request(r#"{"$ref":"https://reuse.test/b"}"#, &resources);
    for _ in 0..2 {
        let (space, _) = space(&request, Vec::new()).unwrap();
        // The same admitted resources in memory, not a copy of them.
        let shared: Vec<_> = space.supplied.iter().map(std::ptr::from_ref).collect();
        let held: Vec<_> = resources
            .supplied()
            .iter()
            .map(std::ptr::from_ref)
            .collect();
        assert_eq!(shared, held);
    }
}

#[test]
fn shared_and_freshly_admitted_resources_project_identically() {
    const META: &str = "https://json-schema.org/draft/2020-12/schema";
    let cases: Vec<(String, Vec<(&str, String)>)> = vec![
        // An ordinary reference into a supplied resource with a nested identifier.
        (
            r#"{"allOf":[{"$ref":"https://reuse.test/a"},{"$ref":"https://reuse.test/nested"}]}"#.into(),
            vec![(
                "https://reuse.test/a",
                r#"{"$defs":{"n":{"$id":"https://reuse.test/nested","minimum":1}},"type":"integer"}"#.into(),
            )],
        ),
        // Two supplied claims to one identifier, a supplied claim to a packaged
        // one, a supplied claim to a contained one, and a missing resource.
        (
            r#"{"anyOf":[{"$ref":"https://reuse.test/dup"},{"$ref":"https://reuse.test/contained"},{"$ref":"https://reuse.test/missing"}]}"#.into(),
            vec![
                ("https://reuse.test/dup", r#"{"type":"null"}"#.into()),
                (
                    "https://reuse.test/other",
                    format!(
                        r#"{{"$defs":{{"d":{{"$id":"https://reuse.test/dup"}},"m":{{"$id":"{META}"}},"c":{{"$id":"https://reuse.test/contained"}}}}}}"#
                    ),
                ),
            ],
        ),
        (format!(r#"{{"$ref":"{META}"}}"#), vec![(META, r#"{"type":"integer"}"#.into())]),
    ];
    let control = std::sync::atomic::AtomicBool::new(false);
    let mut planned = 0;
    for (input, entries) in cases {
        let entries: Vec<_> = entries
            .iter()
            .map(|(uri, text)| (*uri, text.as_str()))
            .collect();
        let resources = set(&entries);
        let request = request(&input, &resources);
        let (shared, _) = space(&request, Vec::new()).unwrap();
        let fresh = embedding::SchemaSpace::new(
            embedding::RawDocument::new(request.document().as_document().value().clone()),
            embedding::Supplied::new(request.supplied_resources().iter().map(|r| {
                embedding::ProjectedResource {
                    uri: r.uri.clone(),
                    document: r.document.clone(),
                }
            }))
            .unwrap(),
        );
        assert_eq!(
            serde_json::to_string(&shared.references(&control)).unwrap(),
            serde_json::to_string(&fresh.references(&control)).unwrap()
        );
        for (id, _) in shared.nodes().filter(|(_, node)| node.source == 0) {
            assert_eq!(program(&shared, id), program(&fresh, id), "{input} {id}");
            planned += 1;
        }
    }
    assert!(planned >= 10, "{planned}");
}
