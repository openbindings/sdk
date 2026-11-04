use super::super::ast;
use super::*;
use openbindings_internal_schema::preparation::Attempt;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn parsed(form: &str) -> Parsed {
    ast::prepare(form, 32, 250, &|| false).unwrap()
}
fn summary(form: &str) -> Summary {
    inspect(parsed(form), &|| false).unwrap().summary
}

#[derive(Default)]
struct Recorder {
    work: AtomicUsize,
    allocation: AtomicUsize,
}
impl Control for Recorder {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn check(&self) -> std::result::Result<(), Failure> {
        self.charge_work(1)
    }
    fn charge_work(&self, n: usize) -> std::result::Result<(), Failure> {
        self.work.fetch_add(n, Relaxed);
        Ok(())
    }
    fn charge_allocation(&self, n: usize) -> std::result::Result<(), Failure> {
        self.check()?;
        self.allocation.fetch_add(n, Relaxed);
        Ok(())
    }
}
#[test]
fn lexical_literal_runs_respect_group_and_class_boundaries() {
    let flat = summary("abc");
    let grouped = summary("a(?:b)c");
    let classed = summary("a[b]c");
    assert_eq!(flat.shape.literal_bytes, 3);
    assert_eq!(grouped.shape.literal_bytes, 3);
    assert_eq!(classed.shape.literal_bytes, 3);
    assert_eq!(flat.work.concat_input, 1);
    assert_eq!(grouped.work.concat_input, 3);
    assert_eq!(classed.work.concat_input, 3);
    assert_eq!(classed.ranges, 1);
    assert!(grouped.nodes > flat.nodes);
}
#[test]
fn smart_collapses_preserve_original_lineage() {
    let erased = summary("(?:[a-z]abc){0}");
    let retained = summary("(?:[a-z]abc){1}");
    assert_eq!(erased.shape.root, Root::Empty);
    assert_eq!(erased.shape.retained_nodes, 1);
    assert_eq!(erased.nodes, retained.nodes);
    assert_eq!(erased.ranges, retained.ranges);
    assert_eq!(erased.literal_bytes, retained.literal_bytes);
    assert_eq!(erased.classes, 1);
    assert!(erased.work.concat_visit > 0);
    assert!(erased.smart_allocation > 0);
    assert_eq!(summary("(?:^){4294967295}").shape.root, Root::Look);
    assert!(!summary("(?:a?){4294967295}").shape.zero_width_only);
}
#[test]
fn attempted_equality_survives_later_nonconcat() {
    let s = summary("ab[c-d]|ab[e-f]|z");
    assert!(s.work.prefix_nodes > 0);
    assert!(s.equality_height > 0);
    assert_eq!(s.prefix_calls, 0);
    assert_eq!(s.work.prefix_split, 0);
}
#[test]
fn wide_inputs_keep_constant_frame_storage() {
    let narrow = Recorder::default();
    drop(inspect(parsed("a?|b?"), &narrow).unwrap());
    let wide = Recorder::default();
    let pattern = std::iter::repeat_n("a?", 5000)
        .collect::<Vec<_>>()
        .join("|");
    drop(inspect(parsed(&pattern), &wide).unwrap());
    assert_eq!(
        narrow.allocation.load(Relaxed),
        wide.allocation.load(Relaxed)
    );
    assert!(wide.work.load(Relaxed) > narrow.work.load(Relaxed));
    let deep = Recorder::default();
    let pattern = format!("{}a{}", "(".repeat(32), ")".repeat(32));
    drop(inspect(parsed(&pattern), &deep).unwrap());
    assert!(deep.allocation.load(Relaxed) > wide.allocation.load(Relaxed));
}
#[test]
fn exact_and_one_below_allowances() {
    for form in ["", "a0000|b0000", "((a|b)*)"] {
        let recorder = Recorder::default();
        drop(inspect(parsed(form), &recorder).unwrap());
        let work = recorder.work.load(Relaxed);
        let allocation = recorder.allocation.load(Relaxed);
        let ast = parsed(form);
        let result = Attempt::run(work, allocation, &|| false, |attempt| inspect(ast, attempt));
        drop(result.unwrap().unwrap());
        let ast = parsed(form);
        let result = Attempt::run(work - 1, allocation, &|| false, |attempt| {
            inspect(ast, attempt)
        });
        assert_eq!(result.unwrap_err().code, "preparation-work-units");
        if allocation > 0 {
            let ast = parsed(form);
            let result = Attempt::run(work, allocation - 1, &|| false, |attempt| {
                inspect(ast, attempt)
            });
            assert_eq!(result.unwrap_err().code, "preparation-allocation-bytes");
        }
    }
}
#[test]
fn checked_arithmetic_and_prior_causes() {
    assert!(matches!(
        checked(Amount::from(usize::MAX) + 1),
        Err(InternalError::Overflow)
    ));
    assert!(matches!(
        checked(Amount::from(usize::MAX) * 2),
        Err(InternalError::Overflow)
    ));
    let s = Summary {
        nodes: usize::MAX,
        ..Summary::default()
    };
    assert!(s.work_sites(8).is_none());
    assert!(s.allocation_sites().is_none());
    assert!(s.work_sites(16).is_none());
    let result = Attempt::run(100, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert_eq!(failure(attempt).code, "preparation-allocation-bytes");
    });
    assert_eq!(result.unwrap_err().code, "preparation-allocation-bytes");
    let result = Attempt::run(100, usize::MAX, &|| false, |attempt| {
        attempt.charge_work(1).unwrap();
        let failed = failure(attempt);
        assert_eq!(failed.code, "preparation-work-units");
        assert!(attempt.check().is_err());
    });
    assert_eq!(result.unwrap_err().code, "preparation-work-units");
}
#[test]
fn cancellation_never_publishes_a_partial_certificate() {
    let ast = parsed("(a|b)*");
    assert!(matches!(
        inspect(ast, &|| true),
        Err(Error::Attempt(Failure {
            reason: FailureReason::Cancelled,
            ..
        }))
    ));
    let counter = AtomicUsize::new(0);
    for boundary in 1..30 {
        counter.store(0, Relaxed);
        let ast = parsed("((((a|b)*)))");
        let cancelled = || counter.fetch_add(1, Relaxed) >= boundary;
        let result = Attempt::run(usize::MAX, usize::MAX, &cancelled, |attempt| {
            inspect(ast, attempt)
        });
        if let Err(failure) = result {
            assert_eq!(failure.reason, FailureReason::Cancelled);
        }
    }
    drop(inspect(parsed("(a|b)*"), &|| false).unwrap());
}
#[test]
fn original_forms_keep_owned_ast_and_complete_schedules() {
    let mut count = 0;
    for line in include_str!("../normalized/tests/reader-inputs.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let Ok(crate::pattern::Reading::Linear(reading)) =
            crate::pattern::read(row["pattern"].as_str().unwrap(), 32, 1024 * 1024)
        else {
            continue;
        };
        let owned = inspect(parsed(&reading.form), &|| false).unwrap();
        assert_eq!(owned.parsed.tree.span().end.offset, reading.form.len());
        assert!(owned.summary.work_sites(8).is_some());
        assert!(owned.summary.work_sites(4).is_some());
        assert!(owned.summary.allocation_sites().is_some());
        assert_eq!(
            owned.work,
            owned
                .summary
                .work_sites(size_of::<usize>())
                .unwrap()
                .into_iter()
                .sum::<usize>()
        );
        assert_eq!(
            owned.allocation,
            owned
                .summary
                .allocation_sites()
                .unwrap()
                .into_iter()
                .sum::<usize>()
        );
        count += 1;
    }
    assert_eq!(count, 1142);
}

#[test]
fn independently_authored_shape_and_site_controls() {
    // These expected records predate this implementation and come from the
    // independently reviewed recursive model, not a snapshot of this walker.
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("tests/independent.json")).unwrap();
    for row in rows {
        let s = summary(row["form"].as_str().unwrap());
        let h = s.shape;
        let w = s.work;
        let expected = &row["expected"];
        let shape = &expected["shape"];
        macro_rules! field {
            ($key:literal,$actual:expr) => {
                assert_eq!(
                    serde_json::json!($actual),
                    shape[$key],
                    "{} field {}",
                    row["id"],
                    $key
                );
            };
        }
        field!("p", h.first_branch_weight);
        field!("m", h.retained_nodes);
        field!("n", s.nodes);
        field!("a", s.alternatives);
        field!("u", s.unary);
        field!("b", s.literal_bytes);
        field!("r", s.ranges);
        field!("k", s.classes);
        field!("front", h.frontier);
        field!("flat_r", h.exposed_ranges);
        field!("all_concat", h.all_concat);
        field!("lit", h.literal_bytes);
        field!("zero", h.zero_width_only);
        field!("nonzero", h.possible_nonempty);
        field!("h", h.height);
        field!("cost", s.smart_allocation);
        field!("prefixes", s.prefix_calls);
        field!("concat_cost", s.concat_allocation);
        field!("alt_cost", s.alternate_allocation);
        let root = if h.root == Root::Alternate {
            "Alt".to_owned()
        } else {
            format!("{:?}", h.root)
        };
        field!("root", root);
        assert_eq!(
            s.maximum_prefix_calls as u64,
            expected["maximum_prefix_calls"].as_u64().unwrap()
        );
        assert_eq!(
            s.equality_height as u64,
            expected["maximum_equality_height"].as_u64().unwrap()
        );
        let counters = serde_json::json!({"concat_input":w.concat_input,"concat_visit":w.concat_visit,"concat_bytes":w.concat_bytes,
            "alt_input":w.alternate_input,"alt_frontier":w.alternate_frontier,"prefix_nodes":w.prefix_nodes,"prefix_bytes":w.prefix_bytes,
            "prefix_ranges":w.prefix_ranges,"prefix_split":w.prefix_split,"sort_pairs":w.sort_pairs,"range_slots":w.range_slots});
        assert_eq!(
            counters, expected["work_counters"],
            "{} work counters",
            row["id"]
        );
        for (word, key) in [(8, "work_sites_native"), (4, "work_sites_wasm_width")] {
            let sites: Vec<usize> = expected[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|site| site[1].as_u64().unwrap() as usize)
                .collect();
            assert_eq!(
                s.work_sites(word).unwrap().as_slice(),
                sites,
                "{} {key}",
                row["id"]
            );
        }
        let sites: Vec<usize> = expected["allocation_sites_native"]
            .as_array()
            .unwrap()
            .iter()
            .map(|site| site[1].as_u64().unwrap() as usize)
            .collect();
        assert_eq!(
            s.allocation_sites().unwrap().as_slice(),
            sites,
            "{} allocation",
            row["id"]
        );
    }
}
