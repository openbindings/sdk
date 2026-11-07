use super::*;
use crate::pattern::{Reading, read};
use openbindings::{NoVerdictReason, WorkControl};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn engine(source: &str) -> PikeVM {
    let Reading::Linear(linear) = read(source, 32, 1 << 20).unwrap() else {
        panic!("expected a supported original pattern")
    };
    assert!(linear.simple.is_none());
    PikeVM::builder()
        .thompson(thompson::Config::new().which_captures(thompson::WhichCaptures::None))
        .build(&linear.form)
        .unwrap()
}

#[test]
fn broad_epsilon_closures_need_more_than_a_state_only_reservation() {
    let control = AtomicBool::new(false);
    for count in [100, 1_000, 10_000] {
        let engine = engine(&"|".repeat(count));
        assert_eq!(engine.get_nfa().states().len(), 4);
        let bytes = reservation(engine.get_nfa()).unwrap();
        let mut graph = Graph::new(bytes, None, 0, &control);
        let mut cache = create(&engine, &mut graph).unwrap();
        for text in ["", "x", "a string"] {
            assert!(engine.is_match(&mut cache, text));
        }
        assert!(
            create(&engine, &mut Graph::new(bytes - 1, None, 0, &control)).is_err(),
            "cache creation must refuse before its backing is requested"
        );
        if count >= 1_000 {
            assert!(bytes > 4 * 256 + 4096);
        }
    }
}

#[test]
fn repeated_searches_keep_their_original_cache_and_matching_semantics() {
    let control = AtomicBool::new(false);
    for (source, positive, negative) in [
        ("a0000|b0000", "b0000", "c0000"),
        ("^(?:a|b?){0,100}z$", "ababz", "abab"),
        ("^(?:é|☃|a)*x$", "☃éax", "☃éa"),
    ] {
        let engine = engine(source);
        let mut graph = Graph::new(reservation(engine.get_nfa()).unwrap(), None, 0, &control);
        let mut cache = create(&engine, &mut graph).unwrap();
        for _ in 0..3 {
            assert!(engine.is_match(&mut cache, positive));
            assert!(!engine.is_match(&mut cache, negative));
        }
    }
}

#[test]
fn cache_work_and_cancellation_refuse_before_construction() {
    struct CancelAt(usize, AtomicUsize);
    impl WorkControl for CancelAt {
        fn is_cancelled(&self) -> bool {
            self.1.fetch_add(1, Ordering::Relaxed) >= self.0
        }
    }
    let engine = engine(&"|".repeat(1_000));
    for checkpoint in 0..2 {
        let control = CancelAt(checkpoint, AtomicUsize::new(0));
        let failure = create(&engine, &mut Graph::new(usize::MAX, None, 0, &control)).unwrap_err();
        assert_eq!(failure.reason, NoVerdictReason::Cancelled);
    }
    let control = AtomicBool::new(false);
    let work = engine.get_nfa().states().len() + 1;
    create(
        &engine,
        &mut Graph::new(usize::MAX, Some(work), 0, &control),
    )
    .unwrap();
    assert!(
        create(
            &engine,
            &mut Graph::new(usize::MAX, Some(work - 1), 0, &control)
        )
        .is_err()
    );
}

#[test]
fn impossible_vector_layouts_refuse_without_reaching_the_dependency() {
    assert!(backing(usize::MAX, 0, 1, 1).is_none());
    assert!(backing(1, usize::MAX, 1, 1).is_none());
    assert!(backing(1, 0, usize::MAX, 1).is_none());
    assert!(backing(1, 0, 1, usize::MAX).is_none());
    // Each multiplication fits usize but would exceed Rust's vector byte limit.
    assert!(backing(isize::MAX as usize / size_of::<StateID>() + 1, 0, 1, 1).is_none());
    assert!(backing(1, isize::MAX as usize / size_of::<usize>() + 1, 1, 1).is_none());
    assert!(backing(1, 0, 1, isize::MAX as usize / (3 * size_of::<usize>())).is_none());
}

#[test]
fn prior_reservations_cannot_wrap_into_an_unlimited_cache_budget() {
    let engine = engine("a0000|b0000");
    let control = AtomicBool::new(false);
    let mut graph = Graph::new(usize::MAX, None, 0, &control);
    graph.charge(usize::MAX).unwrap();
    assert!(create(&engine, &mut graph).is_err());
}
