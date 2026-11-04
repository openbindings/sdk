use super::*;
use openbindings_internal_schema::{FailureReason, preparation::Attempt};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::Relaxed},
};

#[test]
fn joined_factory_owns_one_program_after_source_budget_and_control_drop() {
    let source = r"^(ab|\x{E9}\x{1F600}){1,2}$".to_owned();
    let control = Arc::new(AtomicUsize::new(0));
    let weak = Arc::downgrade(&control);
    let vm = {
        let budget = profile::Budget::default();
        prepare(&source, 1 << 20, &budget, &|| {
            control.fetch_add(1, Relaxed);
            false
        })
        .unwrap()
    };
    drop(source);
    drop(control);
    assert!(weak.upgrade().is_none());
    std::thread::scope(|scope| {
        for (text, expected) in [
            ("ab", true),
            ("é😀ab", true),
            ("xab", false),
            ("ab\n", false),
        ] {
            let vm = &vm;
            scope.spawn(move || assert_eq!(vm.is_match(&mut vm.create_cache(), text), expected));
        }
    });
}

#[test]
fn local_failures_and_shared_failures_keep_separate_meaning() {
    let budget = profile::Budget::default();
    assert!(matches!(
        prepare("a{65535}", 1 << 20, &budget, &|| false),
        Err(Error::ConstructionLimit)
    ));
    assert!(matches!(
        prepare("a?", 1, &budget, &|| false),
        Err(Error::Size)
    ));
    assert!(matches!(
        prepare("*", 1 << 20, &budget, &|| false),
        Err(Error::Representation)
    ));
    budget.check(&|| false).unwrap();
    prepare("a?", 1 << 20, &budget, &|| false).unwrap();
    let result = Attempt::run(0, usize::MAX, &|| false, |control| {
        let result = prepare("a?", 1 << 20, &profile::Budget::default(), control);
        assert!(matches!(
            result,
            Err(Error::Attempt(Failure {
                reason: FailureReason::LimitExceeded,
                ..
            }))
        ));
    });
    assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
}

#[test]
fn cancellation_at_every_factory_boundary_prevents_publication() {
    let calls = AtomicUsize::new(0);
    prepare(
        r"(ab|[\x{E9}-\x{EA}])*",
        1 << 20,
        &profile::Budget::default(),
        &|| {
            calls.fetch_add(1, Relaxed);
            false
        },
    )
    .unwrap();
    let count = calls.load(Relaxed);
    for stop in 0..count {
        calls.store(0, Relaxed);
        let cancel = || calls.fetch_add(1, Relaxed) >= stop;
        let failed = prepare(
            r"(ab|[\x{E9}-\x{EA}])*",
            1 << 20,
            &profile::Budget::default(),
            &cancel,
        );
        assert!(matches!(
            failed,
            Err(Error::Attempt(Failure {
                reason: FailureReason::Cancelled,
                ..
            }))
        ));
    }
}
