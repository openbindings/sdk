use std::path::Path;

type Target = fn(&[u8]) -> bool;

#[test]
fn all_target_bodies_replay_committed_seeds() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    let targets: [(&str, Target); 5] = [
        ("document", openbindings_fuzz::document),
        ("json_value", openbindings_fuzz::json_value),
        ("decimal", openbindings_fuzz::decimal),
        ("pattern", openbindings_fuzz::pattern),
        ("lanes", openbindings_fuzz::lanes),
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
        for path in &paths {
            let data = std::fs::read(path).unwrap();
            assert!(
                data.len() <= openbindings_fuzz::MAX_INPUT,
                "oversize seed {}",
                path.display()
            );
            eprintln!("replay {}", path.display());
            exercised += usize::from(target(&data));
        }
        assert!(
            exercised > 0,
            "{name} seeds never reached a successful input"
        );
        eprintln!(
            "{name}: {} seeds, {exercised} successful inputs",
            paths.len()
        );
    }
}
