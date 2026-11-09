use super::super::ast;
use super::*;
use openbindings_internal_schema::{FailureReason, preparation::Attempt};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn parsed(form: &str) -> Ast {
    ast::prepare(
        form,
        32,
        250,
        &super::super::profile::Budget::default(),
        &|| false,
    )
    .unwrap()
}
fn summary(form: &str) -> Summary {
    inspect(parsed(form), &|| false).unwrap().summary
}

#[test]
fn lineage_survives_smart_collapses_and_later_nonconcat() {
    let erased = summary("(?:[a-z]abc){0}");
    assert_eq!(erased.shape.root, Root::Empty);
    assert_eq!(erased.shape.retained_nodes, 1);
    assert!(erased.pressure.reconstruction_entries > 0);
    assert!(erased.pressure.literal_byte_revisits > 0);
    assert_eq!(summary("(?:^){4294967295}").shape.root, Root::Look);
    assert!(!summary("(?:a?){4294967295}").shape.zero_width_only);
    let later = summary("ab[c-d]|ab[e-f]|z");
    assert!(later.equality_height > 0);
    assert!(later.pressure.reconstruction_entries > 0);
}

#[test]
fn arithmetic_refusal_is_local_but_preserves_a_prior_shared_cause() {
    assert!(matches!(
        checked(Amount::from(usize::MAX) + 1),
        Err(InternalError::Overflow)
    ));
    assert!(matches!(
        checked(Amount::from(usize::MAX) * 2),
        Err(InternalError::Overflow)
    ));
    assert!(matches!(
        map_error(InternalError::Overflow, &|| false),
        Error::ConstructionLimit
    ));
    let result = Attempt::run(100, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert!(matches!(
            map_error(InternalError::Overflow, attempt),
            Error::Attempt(Failure {
                reason: FailureReason::LimitExceeded,
                ..
            })
        ));
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
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
fn original_forms_preserve_ast_and_candidate_admission() {
    let mut count = 0;
    let mut attacks = 0;
    for line in include_str!("../normalized/tests/reader-inputs.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let Ok(crate::pattern::Reading::Linear(reading)) =
            crate::pattern::read(row["pattern"].as_str().unwrap(), 32, 1024 * 1024)
        else {
            continue;
        };
        let parsed = ast::prepare(
            &reading.form,
            33,
            250,
            &super::super::profile::Budget::default(),
            &|| false,
        );
        if matches!(row["id"].as_u64(), Some(1184 | 1185)) {
            assert!(matches!(
                parsed,
                Err(ast::Error::Refused(ast::Refusal::ConstructionLimit))
            ));
            attacks += 1;
            continue;
        }
        let owned = inspect(parsed.unwrap(), &|| false).unwrap();
        assert_eq!(owned.tree.span().end.offset, reading.form.len());
        assert!(
            super::super::profile::admits_translation(&owned.summary.pressure),
            "{}",
            row["id"]
        );
        count += 1;
    }
    assert_eq!((count, attacks), (1140, 2));
}

#[test]
fn independent_shapes_and_structural_pressure_keep_frozen_meaning() {
    // Frozen before this implementation. Old byte/work site arrays in these
    // receipts are historical; only facts used by the new profile are compared.
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("tests/independent.json")).unwrap();
    for row in rows {
        let s = summary(row["form"].as_str().unwrap());
        let h = s.shape;
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
        field!("front", h.frontier);
        field!("flat_r", h.exposed_ranges);
        field!("all_concat", h.all_concat);
        field!("lit", h.literal_bytes);
        field!("zero", h.zero_width_only);
        field!("nonzero", h.possible_nonempty);
        field!("h", h.height);
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
        let w = &expected["work_counters"];
        let count = |key: &str| w[key].as_u64().unwrap() as usize;
        assert_eq!(
            s.pressure.reconstruction_entries,
            count("concat_visit")
                + count("alt_frontier")
                + count("prefix_nodes")
                + count("prefix_split"),
            "{}",
            row["id"]
        );
        assert_eq!(
            s.pressure.literal_byte_revisits,
            count("concat_bytes") + count("prefix_bytes"),
            "{}",
            row["id"]
        );
        assert_eq!(
            s.pressure.candidate_range_slots,
            count("range_slots"),
            "{}",
            row["id"]
        );
    }
}
