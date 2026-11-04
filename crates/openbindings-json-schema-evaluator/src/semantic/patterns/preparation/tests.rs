use super::*;
use openbindings_internal_schema::preparation::Attempt;
#[test]
fn short_unicode_properties_reserve_their_expansion_before_reading() {
    let failed = Attempt::run(1_000_000, 2048, &|| false, |a| {
        Programs::default().add(
            r"\p{L}",
            &Limits::default(),
            &compiler::profile::Budget::default(),
            a,
        )
    })
    .unwrap_err();
    assert_eq!(failed.code, "preparation-sdk-reservation-bytes");
    let (reserved, retained) = Attempt::run(1_000_000, 8 * 1024 * 1024, &|| false, |a| {
        let patterns = Programs::default();
        patterns
            .add(
                r"\p{L}",
                &Limits::default(),
                &compiler::profile::Budget::default(),
                a,
            )
            .unwrap();
        (
            a.allocated(),
            patterns
                .forms
                .borrow()
                .values()
                .flatten()
                .map(|form| form.text.len())
                .sum::<usize>(),
        )
    })
    .unwrap();
    assert!(retained > 10_000);
    assert!(reserved > retained * 4);
}

#[test]
fn literal_ranges_are_charged_again_for_repeated_shorthand_unions() {
    let literals: String = (0..1000)
        .map(|i| char::from_u32(0x1000 + i * 2).unwrap())
        .collect();
    let source = format!("[{literals}{}]", r"\s".repeat(1000));
    let failed = Attempt::run(usize::MAX, 1024 * 1024, &|| false, |a| {
        Programs::default().add(
            &source,
            &Limits::default(),
            &compiler::profile::Budget::default(),
            a,
        )
    })
    .unwrap_err();
    assert_eq!(failed.code, "preparation-sdk-reservation-bytes");
    let (_, allocation) = pattern::preparation_cost(&source).unwrap();
    assert!(allocation > 16_482_598);
}

#[test]
fn completed_normalized_programs_are_shared_without_rebuilding() {
    let programs = Programs::default();
    let budget = compiler::profile::Budget::default();
    let limits = Limits::default();
    let before = compiler::build_count();
    let first = programs.add("a|b", &limits, &budget, &|| false).unwrap();
    let footprint = programs.footprint.get();
    let second = programs
        .add(r"\u0061|b", &limits, &budget, &|| false)
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(compiler::build_count(), before + 1);
    assert_eq!(programs.footprint.get(), footprint);
    assert_eq!(programs.forms.borrow().len(), 1);
}

#[test]
fn local_refusals_and_shared_footprint_exhaustion_have_different_owners() {
    let programs = Programs::default();
    let budget = compiler::profile::Budget::default();
    let limits = Limits::default();
    let local = programs
        .add("a{65535}", &limits, &budget, &|| false)
        .unwrap();
    assert!(matches!(
        local.as_ref(),
        Program::Limited("pattern-size-limit")
    ));
    let repeated = programs
        .add("a{65535}", &limits, &budget, &|| false)
        .unwrap();
    assert!(Arc::ptr_eq(&local, &repeated));
    programs.add("b?", &limits, &budget, &|| false).unwrap();
    programs.check(&|| false).unwrap();

    let baseline = Programs::default();
    baseline
        .add(
            "a?",
            &limits,
            &compiler::profile::Budget::default(),
            &|| false,
        )
        .unwrap();
    let bytes = baseline.footprint.get();
    for (maximum, allowed) in [(bytes, true), (bytes - 1, false)] {
        let programs = Programs::default();
        let limits = Limits {
            total_pattern_memory: maximum,
            ..Limits::default()
        };
        let result = programs.add(
            "a?",
            &limits,
            &compiler::profile::Budget::default(),
            &|| false,
        );
        assert_eq!(result.is_ok(), allowed);
        assert_eq!(programs.check(&|| false).is_ok(), allowed);
        if !allowed {
            assert!(programs.forms.borrow().is_empty());
            assert!(
                matches!(programs.add("[", &limits, &compiler::profile::Budget::default(), &|| false),
                Err(PreparationFailure { kind: PreparationFailureKind::Limit { resource }, .. })
                    if resource == "pattern-total-memory-limit")
            );
        }
    }
}

#[test]
fn reader_refusals_never_publish_partial_normalized_keys() {
    let limits = Limits {
        pattern_bytes: 3,
        pattern_depth: 0,
        ..Limits::default()
    };
    let programs = Programs::default();
    let budget = compiler::profile::Budget::default();
    for (source, code) in [
        ("abcd", "pattern-byte-limit"),
        ("(a)", "pattern-depth-limit"),
    ] {
        assert!(
            matches!(programs.add(source, &limits, &budget, &|| false).unwrap().as_ref(), Program::Limited(c) if *c==code)
        );
        assert!(programs.forms.borrow().is_empty());
    }
    let simple = programs.add("^a", &limits, &budget, &|| false).unwrap();
    assert!(matches!(simple.as_ref(), Program::Simple { .. }));
}

#[test]
fn interrupted_insertions_never_become_reusable_programs() {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    let calls = AtomicUsize::new(0);
    let limits = Limits::default();
    Programs::default()
        .add(
            "(ab|cd)+",
            &limits,
            &compiler::profile::Budget::default(),
            &|| {
                calls.fetch_add(1, Relaxed);
                false
            },
        )
        .unwrap();
    let total = calls.load(Relaxed);
    for at in 0..total {
        let programs = Programs::default();
        calls.store(0, Relaxed);
        let result = programs.add(
            "(ab|cd)+",
            &limits,
            &compiler::profile::Budget::default(),
            &|| calls.fetch_add(1, Relaxed) >= at,
        );
        assert!(
            matches!(
                result,
                Err(PreparationFailure {
                    kind: PreparationFailureKind::Cancelled,
                    ..
                })
            ),
            "{at}"
        );
        assert!(
            programs.forms.borrow().is_empty(),
            "interrupted publication at {at}"
        );
    }
}

#[test]
fn hash_collisions_preserve_exact_forms_sharing_and_interrupted_siblings() {
    let limits = Limits::default();
    let programs = Programs::default();
    programs.forced_hash.set(Some(0));
    let budget = compiler::profile::Budget::default();
    let first = programs.add("^a$", &limits, &budget, &|| false).unwrap();
    let second = programs.add("^b$", &limits, &budget, &|| false).unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(programs.forms.borrow()[&(3, 0)].len(), 2);
    for source in ["^a$", r"^\x61$", r"^\u0061$"] {
        assert!(Arc::ptr_eq(
            &first,
            &programs.add(source, &limits, &budget, &|| false).unwrap()
        ));
    }
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    let calls = AtomicUsize::new(0);
    programs
        .add("^c$", &limits, &budget, &|| {
            calls.fetch_add(1, Relaxed);
            false
        })
        .unwrap();
    let total = calls.load(Relaxed);
    for at in 0..total {
        let programs = Programs::default();
        programs.forced_hash.set(Some(0));
        let budget = compiler::profile::Budget::default();
        let first = programs.add("^a$", &limits, &budget, &|| false).unwrap();
        let second = programs.add("^b$", &limits, &budget, &|| false).unwrap();
        calls.store(0, Relaxed);
        assert!(
            matches!(
                programs.add("^c$", &limits, &budget, &|| calls.fetch_add(1, Relaxed)
                    >= at),
                Err(PreparationFailure {
                    kind: PreparationFailureKind::Cancelled,
                    ..
                })
            ),
            "{at}"
        );
        assert_eq!(programs.forms.borrow()[&(3, 0)].len(), 2, "{at}");
        assert!(Arc::ptr_eq(
            &first,
            &programs.add("^a$", &limits, &budget, &|| false).unwrap()
        ));
        assert!(Arc::ptr_eq(
            &second,
            &programs.add("^b$", &limits, &budget, &|| false).unwrap()
        ));
    }
}
