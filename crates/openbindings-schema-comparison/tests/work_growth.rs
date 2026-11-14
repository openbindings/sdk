//! Cost guarantees measured only through the public comparison work allowance.
use openbindings::{JsonValue, WorkControl};
use openbindings_schema_comparison::{
    ComparisonFailureKind, ComparisonLimits, Direction, SchemaInput, Verdict, compare,
    compare_with_control,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn schema(text: String) -> SchemaInput {
    let value = JsonValue::parse(text).unwrap();
    SchemaInput::new(value.clone(), value)
}

fn names(n: usize) -> String {
    (0..n)
        .rev()
        .map(|i| format!("\"value{i:08}\""))
        .collect::<Vec<_>>()
        .join(",")
}

fn minimum_work(target: &SchemaInput, candidate: &SchemaInput) -> usize {
    let completes = |work| match compare(
        Direction::Input,
        target,
        candidate,
        &ComparisonLimits {
            max_work: Some(work),
            ..Default::default()
        },
    ) {
        Ok(result) => {
            assert_eq!(result.verdict(), Verdict::Compatible);
            true
        }
        Err(error) => {
            assert_eq!(error.kind(), ComparisonFailureKind::LimitExceeded("work"));
            false
        }
    };
    assert!(!completes(0));
    let (mut low, mut high) = (0, 1usize);
    while !completes(high) {
        high = high.checked_mul(2).expect("work search overflow");
    }
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if completes(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}

fn assert_growth(case: &str, inputs: impl Fn(usize) -> (SchemaInput, SchemaInput)) {
    let mut previous = None;
    for n in [1_000, 4_000, 16_000] {
        let (target, candidate) = inputs(n);
        let work = minimum_work(&target, &candidate);
        println!("{case}: n={n}, minimum max_work={work}");
        if let Some(before) = previous {
            // With fixed-width literals, 4n log2(4n)/(n log2(n)) <= 4.81
            // here. 8 leaves room for constants and tree shape; n^2 grows 16x.
            assert!(work <= 8 * before, "{case}: {work} > 8 * {before}");
        }
        previous = Some(work);
    }
}

#[test]
fn directional_enum_membership_work_grows_no_faster_than_n_log_n() {
    assert_growth("enum subset", |n| {
        (
            schema(format!(r#"{{"enum":[{}]}}"#, names(n))),
            schema(format!(r#"{{"enum":[{}]}}"#, names(2 * n))),
        )
    });
}

#[test]
fn directional_const_against_enum_work_grows_no_faster_than_n_log_n() {
    assert_growth("const against enum", |n| {
        (
            schema(r#"{"const":"value00000000"}"#.into()),
            schema(format!(r#"{{"enum":[{}]}}"#, names(2 * n))),
        )
    });
}

#[test]
fn directional_required_subset_work_grows_no_faster_than_n_log_n() {
    assert_growth("required subset", |n| {
        (
            schema(format!(
                r#"{{"type":"object","required":[{}]}}"#,
                names(2 * n)
            )),
            schema(format!(r#"{{"type":"object","required":[{}]}}"#, names(n))),
        )
    });
}

#[test]
fn large_directional_comparison_returns_cancelled_with_bounded_extra_polls() {
    struct CancelOnPoll {
        polls: AtomicUsize,
        k: usize,
    }
    impl WorkControl for CancelOnPoll {
        fn is_cancelled(&self) -> bool {
            self.polls.fetch_add(1, Ordering::Relaxed) + 1 >= self.k
        }
    }
    for n in [1_000, 4_000, 16_000] {
        let target = schema(format!(r#"{{"enum":[{}]}}"#, names(n)));
        let candidate = schema(format!(r#"{{"enum":[{}]}}"#, names(2 * n)));
        for k in [1, 16, 64] {
            let control = CancelOnPoll {
                polls: AtomicUsize::new(0),
                k,
            };
            let result = compare_with_control(
                Direction::Input,
                &target,
                &candidate,
                &ComparisonLimits::default(),
                &control,
            );
            assert!(
                matches!(result, Err(error) if error.kind() == ComparisonFailureKind::Cancelled)
            );
            let polls = control.polls.load(Ordering::Relaxed);
            println!(
                "comparison: n={n}, k={k}, polls={polls}, after={}",
                polls - k
            );
            // At most four cleanup/final-wrapper polls after the first true response.
            assert!(polls - k <= 4);
        }
    }
}
