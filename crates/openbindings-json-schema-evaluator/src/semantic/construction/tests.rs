use super::*;
use openbindings_internal_schema::preparation::Attempt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn input() -> serde_json::Value {
    serde_json::from_str(include_str!("shared-meta-input.json")).unwrap()
}

fn build_environment(input: &serde_json::Value) -> SchemaEnvironment {
    let document = Document::parse(input["document"].to_string())
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let supplied = ExternalSchemas::new(input["resources"].as_object().unwrap().iter().map(
        |(uri, value)| ExternalSchema {
            uri: uri.clone(),
            document: JsonValue::parse(value.to_string()).unwrap(),
        },
    ))
    .unwrap();
    document
        .schema("entry")
        .unwrap()
        .environment(
            &supplied,
            &PreparationLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn prepare(input: &serde_json::Value) -> Result<Plan, PreparationFailure> {
    Plan::new(
        build_environment(input),
        Limits::default(),
        PreparationLimits::default(),
        &|| false,
    )
}

fn supplied_node(environment: &SchemaEnvironment, uri: &str) -> usize {
    environment
        .space()
        .query(environment.entry(), uri)
        .unwrap()
        .node
}

fn positive(construction: &Construction<'_>, node: usize) -> bool {
    let number = construction.environment.space().node_number(node);
    let width = usize::BITS as usize;
    construction.syntax[number / width].get() & (1 << (number % width)) != 0
}

#[derive(Default)]
struct Recording {
    work: AtomicUsize,
    allocations: AtomicUsize,
    bytes: AtomicUsize,
}
impl Control for Recording {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn charge_work(&self, units: usize) -> Result<(), openbindings_internal_schema::Failure> {
        self.work.fetch_add(units, Ordering::Relaxed);
        Ok(())
    }
    fn charge_allocation(&self, bytes: usize) -> Result<(), openbindings_internal_schema::Failure> {
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn a_shared_positive_is_only_local_syntax_and_hits_remove_actual_checker_work() {
    let environment = build_environment(&input());
    let helper = supplied_node(&environment, "https://facts.test/helper");
    let recording = Recording::default();
    let construction = Construction::new(environment, Limits::default(), &recording).unwrap();
    assert!(!positive(&construction, helper));
    assert!(construction.check_standard(helper).unwrap());
    let work = recording.work.load(Ordering::Relaxed);
    let allocations = recording.allocations.load(Ordering::Relaxed);
    assert!(positive(&construction, helper));
    assert!(construction.check_standard(helper).unwrap());
    assert_eq!(recording.work.load(Ordering::Relaxed), work + 1);
    assert_eq!(recording.allocations.load(Ordering::Relaxed), allocations);
    let plan = Plan::at(
        &construction,
        construction.environment.entry(),
        PreparationLimits::default(),
    )
    .unwrap();
    meta::validate(&plan, &construction).unwrap();
    for (value, valid) in [("1", true), ("-1", false), ("false", false)] {
        let outcome = plan.evaluate(
            &JsonValue::parse(value).unwrap(),
            Detail::Outcome,
            &AtomicBool::new(false),
        );
        assert_eq!(matches!(outcome, ValueOutcome::Satisfies), valid);
    }
}

#[test]
fn cached_meta_helpers_still_admit_unused_children_and_actual_consumers() {
    prepare(&input()).unwrap();
    for patch in [
        serde_json::json!({"$defs":{"unused":{"type":17}}}),
        serde_json::json!({"contentSchema":{"type":17}}),
        serde_json::json!({"then":{"type":17}}),
        serde_json::json!({"else":{"type":17}}),
        serde_json::json!({"not":{}}),
    ] {
        let mut row = input();
        row["resources"]["https://facts.test/helper"]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let result = prepare(&row);
        assert!(
            matches!(result, Err(ref failure) if failure.kind == PreparationFailureKind::InvalidExternalSchema),
            "{patch}"
        );
    }
    let mut row = input();
    row["resources"]["https://facts.test/helper"] = serde_json::json!(false);
    assert!(
        matches!(prepare(&row), Err(f) if f.kind == PreparationFailureKind::InvalidExternalSchema)
    );
}

#[test]
fn cached_parent_still_discovers_custom_dependencies_and_obligation_cycles() {
    let mut row = input();
    row["resources"]["https://facts.test/nested-meta"] = serde_json::json!({
        "$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true},
        "required":["marker"]
    });
    row["resources"]["https://facts.test/helper"]["$defs"] = serde_json::json!({
        "unused":{"$id":"https://facts.test/nested", "$schema":"https://facts.test/nested-meta"}
    });
    assert!(
        matches!(prepare(&row), Err(f) if f.kind == PreparationFailureKind::InvalidExternalSchema)
    );
    row["resources"]["https://facts.test/helper"]["$defs"]["unused"]["marker"] = true.into();
    prepare(&row).unwrap();
    let mut row = input();
    row["resources"]["https://facts.test/m1"]["allOf"] =
        serde_json::json!([{"$ref":"https://facts.test/c1"}]);
    assert!(
        matches!(prepare(&row), Err(f) if f.kind == PreparationFailureKind::UnsupportedCapability { capability: "recursive-meta-obligation".into() })
    );
}

#[test]
fn node_identity_profile_and_new_environments_bound_the_facts() {
    let mut row = input();
    row["resources"]["https://facts.test/old"] = serde_json::json!({
        "$schema":"http://json-schema.org/draft-07/schema#", "properties":{"child":{"items":[true]}}
    });
    row["resources"]["https://facts.test/new"] =
        serde_json::json!({"properties":{"child":{"items":[true]}}});
    let environment = build_environment(&row);
    let old = supplied_node(&environment, "https://facts.test/old#/properties/child");
    let new = supplied_node(&environment, "https://facts.test/new#/properties/child");
    let helper = supplied_node(&environment, "https://facts.test/helper");
    let custom = supplied_node(&environment, "https://facts.test/c1");
    let construction = Construction::new(environment, Limits::default(), &|| false).unwrap();
    assert!(construction.check_standard(old).unwrap());
    for _ in 0..2 {
        assert!(!construction.check_standard(new).unwrap());
        assert!(!positive(&construction, new));
        assert!(construction.check_standard(custom).is_err());
        assert!(!positive(&construction, custom));
    }
    assert!(construction.check_standard(helper).unwrap());
    row["resources"]["https://facts.test/helper"]["required"] = false.into();
    let replacement =
        Construction::new(build_environment(&row), Limits::default(), &|| false).unwrap();
    let changed = supplied_node(replacement.environment(), "https://facts.test/helper");
    assert!(!positive(&replacement, changed));
    assert!(!replacement.check_standard(changed).unwrap());
}

#[test]
fn packaged_identifiers_map_to_dense_distinct_local_fact_bits() {
    let environment = build_environment(&input());
    let construction = Construction::new(environment, Limits::default(), &|| false).unwrap();
    let space = construction.environment.space();
    let ids: Vec<_> = space.nodes().map(|(id, _)| id).collect();
    assert!(ids.iter().any(|id| *id >= 1 << (usize::BITS - 1)));
    for (number, node) in ids.iter().enumerate() {
        assert_eq!(space.node_number(*node), number);
        assert!(!positive(&construction, *node));
        let width = usize::BITS as usize;
        let word = &construction.syntax[number / width];
        word.set(word.get() | (1 << (number % width)));
        assert!(positive(&construction, *node));
    }
    assert_eq!(ids.len(), space.node_count());
}

struct Stop {
    checks: AtomicUsize,
    at: usize,
}
impl Control for Stop {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::Relaxed) >= self.at
    }
}

#[test]
fn cancelled_misses_cannot_publish_and_hits_still_check_the_attempt() {
    let environment = build_environment(&input());
    let node = supplied_node(&environment, "https://facts.test/helper");
    Construction::new(environment.clone(), Limits::default(), &|| false)
        .unwrap()
        .check_standard(node)
        .unwrap();
    let count = Stop {
        checks: AtomicUsize::new(0),
        at: usize::MAX,
    };
    let construction = Construction::new(environment.clone(), Limits::default(), &count).unwrap();
    count.checks.store(0, Ordering::Relaxed);
    construction.check_standard(node).unwrap();
    let boundaries = count.checks.load(Ordering::Relaxed);
    for at in 0..boundaries {
        let stop = Stop {
            checks: AtomicUsize::new(0),
            at: usize::MAX,
        };
        let mut construction =
            Construction::new(environment.clone(), Limits::default(), &stop).unwrap();
        let stop = Stop {
            checks: AtomicUsize::new(0),
            at,
        };
        construction.control = &stop;
        assert!(construction.check_standard(node).is_err(), "{at}");
        assert!(!positive(&construction, node), "{at}");
    }
    let stop = Stop {
        checks: AtomicUsize::new(0),
        at: 0,
    };
    let mut construction = Construction::new(environment, Limits::default(), &|| false).unwrap();
    construction.check_standard(node).unwrap();
    construction.control = &stop;
    assert!(construction.check_standard(node).is_err());
}

#[test]
fn table_backing_and_shared_work_have_exact_sticky_boundaries() {
    for words in [0, 1, 2, 3, 32] {
        let bytes = words * size_of::<Cell<usize>>();
        let run = |work, bytes| {
            Attempt::run(work, bytes, &|| false, |attempt| {
                allocate_words(words, attempt)
            })
        };
        assert_eq!(run(usize::MAX, bytes).unwrap().unwrap().len(), words);
        if bytes != 0 {
            assert_eq!(
                run(usize::MAX, bytes - 1).unwrap_err().code,
                "preparation-sdk-reservation-bytes"
            );
        }
        let (mut lo, mut hi) = (0, words + 64);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if run(mid, bytes).is_ok_and(|result| result.is_ok()) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        run(lo, bytes).unwrap().unwrap();
        assert_eq!(
            run(lo - 1, bytes).unwrap_err().code,
            "preparation-sdk-work-units"
        );
    }
    for words in [1, usize::MAX] {
        assert_eq!(
            Attempt::run(usize::MAX, usize::MAX, &|| false, |parent| {
                let _ = Attempt::run(usize::MAX, 0, parent, |child| allocate_words(words, child));
                assert!(parent.check().is_err());
            })
            .unwrap_err()
            .code,
            "preparation-sdk-reservation-bytes"
        );
    }
}

#[test]
fn nested_plans_cannot_renew_the_outer_pattern_construction_allowance() {
    let environment = build_environment(&input());
    let construction = Construction::new(environment, Limits::default(), &|| false).unwrap();
    let budget = construction.pattern_budget();
    for _ in 0..7 {
        budget.nfa(0, 1_048_576, 0, construction.control()).unwrap();
    }
    let plan = Plan::at(
        &construction,
        construction.environment.entry(),
        PreparationLimits::default(),
    )
    .unwrap();
    meta::validate(&plan, &construction).unwrap();
    // The same owner remains after root and selected-meta preparation.
    construction
        .pattern_budget()
        .nfa(0, 1_048_576, 0, construction.control())
        .unwrap();
    construction.finish().unwrap();
    let _ignored = construction
        .pattern_budget()
        .nfa(0, 1, 0, construction.control());
    assert!(
        matches!(construction.finish().unwrap_err().kind, PreparationFailureKind::Limit {resource} if resource == "pattern-construction-helper-payload")
    );
    // A genuinely separate outer attempt starts fresh.
    Construction::new(build_environment(&input()), Limits::default(), &|| false)
        .unwrap()
        .finish()
        .unwrap();
}

#[test]
fn actual_root_and_selected_meta_programs_share_one_index_and_aggregate() {
    for reverse in [false, true] {
        for prewarm in [false, true] {
            for remaining in [8, 7] {
                let mut row = input();
                row["document"]["schemas"]["entry"]["pattern"] = "r?".into();
                if reverse {
                    row["document"]["schemas"]["entry"]["allOf"]
                        .as_array_mut()
                        .unwrap()
                        .reverse();
                }
                for (uri, source) in [("m1", "m?"), ("m2", "n?"), ("helper", "h?")] {
                    row["resources"][format!("https://facts.test/{uri}")]["pattern"] =
                        source.into();
                }
                let environment = build_environment(&row);
                let entry = environment.entry();
                let construction =
                    Construction::new(environment, Limits::default(), &|| false).unwrap();
                if prewarm {
                    for uri in ["m1", "m2", "helper"] {
                        let node = supplied_node(
                            construction.environment(),
                            &format!("https://facts.test/{uri}"),
                        );
                        assert!(construction.check_standard(node).unwrap());
                    }
                }
                // Each actual optional literal has independently fixed helper
                // score two. Leave space for exactly four unique programs.
                let budget = construction.pattern_budget();
                for _ in 0..7 {
                    budget.nfa(0, 1_048_576, 0, construction.control()).unwrap();
                }
                budget
                    .nfa(0, 1_048_576 - remaining, 0, construction.control())
                    .unwrap();
                let before = patterns::compiler::build_count();
                let root = Plan::at(&construction, entry, PreparationLimits::default()).unwrap();
                let result = meta::validate(&root, &construction);
                assert_eq!(
                    patterns::compiler::build_count() - before,
                    if remaining == 8 { 4 } else { 3 }
                );
                if remaining == 8 {
                    result.unwrap();
                    construction.finish().unwrap();
                    // A second selected-meta walk hits completed programs even
                    // though no helper admission remains.
                    meta::validate(&root, &construction).unwrap();
                    assert_eq!(patterns::compiler::build_count() - before, 4);
                } else {
                    for error in [result.unwrap_err(), construction.finish().unwrap_err()] {
                        assert!(
                            matches!(error.kind,PreparationFailureKind::Limit{resource}if resource=="pattern-construction-helper-payload")
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn selected_meta_demands_a_local_pattern_limit_only_on_a_string() {
    for title in [
        None,
        Some(serde_json::json!(17)),
        Some(serde_json::json!("text")),
    ] {
        let mut row = input();
        row["resources"]["https://facts.test/m1"]["properties"] =
            serde_json::json!({"title":{"pattern":"a{65535}"}});
        if let Some(title) = &title {
            row["resources"]["https://facts.test/c1"]["title"] = title.clone();
        }
        let result = prepare(&row);
        if title.as_ref().is_some_and(serde_json::Value::is_string) {
            assert!(
                matches!(result,Err(PreparationFailure{kind:PreparationFailureKind::Limit{resource},..})if resource=="pattern-size-limit")
            );
        } else {
            result.unwrap();
        }
    }
}
