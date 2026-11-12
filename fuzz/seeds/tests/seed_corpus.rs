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
