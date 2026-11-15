use std::path::Path;

type Target = fn(&[u8]) -> bool;

#[test]
fn all_target_bodies_replay_committed_seeds() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    let targets: [(&str, Target); 8] = [
        ("document", openbindings_fuzz::document),
        ("json_value", openbindings_fuzz::json_value),
        ("decimal", openbindings_fuzz::decimal),
        ("pattern", openbindings_fuzz::pattern),
        ("lanes", openbindings_fuzz::lanes),
        ("comparison", openbindings_fuzz::comparison),
        ("edit", openbindings_fuzz::edit),
        ("carriers", openbindings_fuzz::carriers),
    ];
    for (name, target) in targets {
        let mut paths: Vec<_> = std::fs::read_dir(corpus.join(name))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "empty corpus for {name}");
        let mut exercised = 0;
        // The carriers target multiplexes decoders by first byte; each needs
        // its own successful seed.
        let mut admitted = [false; openbindings_fuzz::CARRIER_COUNT];
        for path in &paths {
            let data = std::fs::read(path).unwrap();
            assert!(
                data.len() <= openbindings_fuzz::MAX_INPUT,
                "oversize seed {}",
                path.display()
            );
            eprintln!("replay {}", path.display());
            let success = target(&data);
            exercised += usize::from(success);
            if name == "carriers" && success {
                admitted[usize::from(data[0]) % admitted.len()] = true;
            }
        }
        assert!(
            exercised > 0,
            "{name} seeds never reached a successful input"
        );
        if name == "carriers" {
            for (selector, admitted) in admitted.into_iter().enumerate() {
                assert!(
                    admitted,
                    "carrier decoder {selector} has no successful seed"
                );
            }
        }
        eprintln!(
            "{name}: {} seeds, {exercised} successful inputs",
            paths.len()
        );
    }
}

#[test]
fn relationship_seeds_reach_every_located_or_missing_row_category() {
    use openbindings::{Document, JsonValue, Pointer};
    use openbindings_source_pull::{
        PullLimits, PullRequest, PullResult, PullValidationError, RelationshipErrorKind as Kind,
    };
    use std::sync::atomic::AtomicBool;

    let original = Document::parse(include_str!("../../src/relationship_document.json"))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let request = PullRequest::new(original, "s");
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/carriers");
    for (name, kind, pointer) in [
        ("identifier", Kind::Identifier, Some("/report/added/1")),
        ("key-collision", Kind::KeyCollision, Some("/report/added/1")),
        (
            "binding-source",
            Kind::BindingSource,
            Some("/report/added/1"),
        ),
        (
            "added-coverage",
            Kind::AddedCoverage,
            Some("/report/added/1"),
        ),
        (
            "binding-operation",
            Kind::BindingOperation,
            Some("/report/added/1"),
        ),
        ("reuse", Kind::Reuse, Some("/report/added/1")),
        ("existing-coverage", Kind::ExistingCoverage, None),
        (
            "existing-target",
            Kind::ExistingTarget,
            Some("/report/existing/1"),
        ),
        (
            "reference-target",
            Kind::ReferenceTarget,
            Some("/report/existing/1"),
        ),
        (
            "omitted-target",
            Kind::OmittedTarget,
            Some("/report/omitted/1"),
        ),
        (
            "duplicate-target",
            Kind::DuplicateTarget,
            Some("/report/existing/1"),
        ),
        ("uses", Kind::Uses, Some("/report/added/1")),
    ] {
        let bytes = std::fs::read(corpus.join(format!("relationship-{name}.bin"))).unwrap();
        assert!(openbindings_fuzz::carriers(&bytes));
        let result = PullResult::from_json(JsonValue::parse(&bytes[1..]).unwrap()).unwrap();
        let error = result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap_err();
        let PullValidationError::Relationship(error) = error else {
            panic!("relationship seed {name}")
        };
        assert_eq!(error.kind(), kind, "{name}");
        assert_eq!(error.pointer().and_then(Pointer::as_str), pointer, "{name}");
    }
}
