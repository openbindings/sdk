use super::*;
use openbindings_internal_schema::preparation::Attempt;

#[test]
fn literal_handoff_keeps_exact_budgets_cancellation_and_collision_identity() {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    let programs = Programs::default();
    programs.forced_hash.set(Some(0));
    let budget = compiler::profile::Budget::default();
    let limits = Limits::default();
    for (index, source) in ["^a$", "^b$"].into_iter().enumerate() {
        let program = programs.add(source, &limits, &budget, &|| false).unwrap();
        // Pay for the scan, Reader reservation, Attempt checks and every
        // exact candidate comparison in the forced fingerprint group.
        let work = 4 * source.len() + 515 + (index + 1) * (source.len() + 1);
        let allocation = 2 * source.len() + 512;
        let read = |work, allocation| {
            Attempt::run(work, allocation, &|| false, |a| {
                programs.compiled_reading(source, &limits, a)
            })
        };
        let reading = read(work, allocation).unwrap().unwrap().unwrap();
        let Program::Simple { matcher, .. } = program.as_ref() else {
            panic!("simple matcher expected")
        };
        assert!(Arc::ptr_eq(matcher, reading.simple.as_ref().unwrap()));
        assert_eq!(
            read(work - 1, allocation).err().unwrap().code,
            "preparation-sdk-work-units"
        );
        assert_eq!(
            read(work, allocation - 1).err().unwrap().code,
            "preparation-sdk-reservation-bytes"
        );
        let calls = AtomicUsize::new(0);
        programs
            .compiled_reading(source, &limits, &|| {
                calls.fetch_add(1, Relaxed);
                false
            })
            .unwrap();
        let total = calls.load(Relaxed);
        for at in 0..total {
            calls.store(0, Relaxed);
            assert!(matches!(
                programs.compiled_reading(source, &limits, &|| calls.fetch_add(1, Relaxed) >= at),
                Err(PreparationFailure {
                    kind: PreparationFailureKind::Cancelled,
                    ..
                })
            ));
        }
        let retry = programs
            .compiled_reading(source, &limits, &|| false)
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(matcher, retry.simple.as_ref().unwrap()));
    }
}

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

#[test]
fn authored_hits_skip_reader_reservations_but_keep_reader_policy() {
    let programs = Programs::default();
    let budget = compiler::profile::Budget::default();
    let limits = Limits::default();
    let first = programs.add(r"\S", &limits, &budget, &|| false).unwrap();
    let before = compiler::build_count();
    let repeated = Attempt::run(1000, 0, &|| false, |a| {
        programs.add(r"\S", &limits, &budget, a).unwrap()
    })
    .unwrap();
    assert!(Arc::ptr_eq(&first, &repeated));
    assert_eq!(compiler::build_count(), before);
    let limited = programs
        .add(
            r"\S",
            &Limits {
                pattern_bytes: 16,
                ..limits.clone()
            },
            &budget,
            &|| false,
        )
        .unwrap();
    assert!(matches!(
        limited.as_ref(),
        Program::Limited("pattern-size-limit")
    ));
    let first = programs.add("(a)", &limits, &budget, &|| false).unwrap();
    let limited = programs
        .add(
            "(a)",
            &Limits {
                pattern_depth: 0,
                ..limits
            },
            &budget,
            &|| false,
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &limited));
    assert!(matches!(
        limited.as_ref(),
        Program::Limited("pattern-depth-limit")
    ));
}

#[test]
fn interrupted_source_alias_preserves_healthy_normalized_owner_and_collision() {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    let seed = || {
        let programs = Programs::default();
        programs.forced_hash.set(Some(0));
        let budget = compiler::profile::Budget::default();
        let first = programs
            .add("[ab]", &Limits::default(), &budget, &|| false)
            .unwrap();
        let other = programs
            .add("[cd]", &Limits::default(), &budget, &|| false)
            .unwrap();
        (programs, budget, first, other)
    };
    let (programs, budget, first, _) = seed();
    let calls = AtomicUsize::new(0);
    let alias = programs
        .add("[ba]", &Limits::default(), &budget, &|| {
            calls.fetch_add(1, Relaxed);
            false
        })
        .unwrap();
    assert!(Arc::ptr_eq(&first, &alias));
    let total = calls.load(Relaxed);
    for at in 0..total {
        let (programs, budget, first, other) = seed();
        calls.store(0, Relaxed);
        assert!(
            matches!(
                programs.add("[ba]", &Limits::default(), &budget, &|| calls
                    .fetch_add(1, Relaxed)
                    >= at),
                Err(PreparationFailure {
                    kind: PreparationFailureKind::Cancelled,
                    ..
                })
            ),
            "{at}"
        );
        assert_eq!(
            programs
                .sources
                .borrow()
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            2,
            "{at}"
        );
        for (source, owner) in [("[ab]", &first), ("[cd]", &other)] {
            let hit = programs
                .add(source, &Limits::default(), &budget, &|| false)
                .unwrap();
            assert!(Arc::ptr_eq(owner, &hit), "{at}/{source}");
        }
    }
}
