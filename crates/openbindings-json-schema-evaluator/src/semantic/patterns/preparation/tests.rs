use super::*;
use openbindings_internal_schema::preparation::Attempt;

#[test]
fn literal_handoff_keeps_exact_budgets_cancellation_and_collision_identity() {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    let programs = Programs::default();
    programs.forced_hash.set(Some(0));
    let budget = compiler::profile::Budget::default();
    let limits = Limits::default();
    for source in ["^a$", "^b$"] {
        programs.add(source, &limits, &budget, &|| false).unwrap();
        programs
            .compiled_reading(source, &limits, &|| false)
            .unwrap()
            .unwrap();
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
        programs
            .compiled_reading(source, &limits, &|| false)
            .unwrap()
            .unwrap();
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
    Attempt::run(1_000_000, 8 * 1024 * 1024, &|| false, |a| {
        let patterns = Programs::default();
        patterns
            .add(
                r"\p{L}",
                &Limits::default(),
                &compiler::profile::Budget::default(),
                a,
            )
            .unwrap();
    })
    .unwrap();
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
    programs
        .add("a{65535}", &limits, &budget, &|| false)
        .unwrap();
    programs.add("b?", &limits, &budget, &|| false).unwrap();
    programs.check(&|| false).unwrap();

    for (maximum, allowed) in [(0, false), (usize::MAX, true)] {
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
    }
    programs.add("^a", &limits, &budget, &|| false).unwrap();
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
    }
}

#[test]
fn hash_collisions_preserve_exact_forms_sharing_and_interrupted_siblings() {
    let limits = Limits::default();
    let programs = Programs::default();
    programs.forced_hash.set(Some(0));
    let budget = compiler::profile::Budget::default();
    programs.add("^a$", &limits, &budget, &|| false).unwrap();
    programs.add("^b$", &limits, &budget, &|| false).unwrap();
    for source in ["^a$", r"^\x61$", r"^\u0061$"] {
        programs.add(source, &limits, &budget, &|| false).unwrap();
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
        programs.add("^a$", &limits, &budget, &|| false).unwrap();
        programs.add("^b$", &limits, &budget, &|| false).unwrap();
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
        programs.add("^a$", &limits, &budget, &|| false).unwrap();
        programs.add("^b$", &limits, &budget, &|| false).unwrap();
    }
}

#[test]
fn authored_hits_skip_reader_reservations_but_keep_reader_policy() {
    let programs = Programs::default();
    let budget = compiler::profile::Budget::default();
    let limits = Limits::default();
    programs.add(r"\S", &limits, &budget, &|| false).unwrap();
    programs.add(r"\S", &limits, &budget, &|| false).unwrap();
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
    programs.add("(a)", &limits, &budget, &|| false).unwrap();
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
        programs
            .add("[ab]", &Limits::default(), &budget, &|| false)
            .unwrap();
        programs
            .add("[cd]", &Limits::default(), &budget, &|| false)
            .unwrap();
        (programs, budget)
    };
    let (programs, budget) = seed();
    let calls = AtomicUsize::new(0);
    programs
        .add("[ba]", &Limits::default(), &budget, &|| {
            calls.fetch_add(1, Relaxed);
            false
        })
        .unwrap();
    let total = calls.load(Relaxed);
    for at in 0..total {
        let (programs, budget) = seed();
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
        for source in ["[ab]", "[cd]"] {
            programs
                .add(source, &Limits::default(), &budget, &|| false)
                .unwrap();
        }
    }
}
