use super::*;
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

#[test]
fn a_shared_positive_is_only_local_syntax_and_hits_remove_actual_checker_work() {
    let environment = build_environment(&input());
    let helper = supplied_node(&environment, "https://facts.test/helper");
    let healthy = || false;
    let construction = Construction::new(environment, Limits::default(), &healthy).unwrap();
    assert!(construction.check_standard(helper).unwrap());
    assert!(construction.check_standard(helper).unwrap());
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
        assert!(construction.check_standard(custom).is_err());
    }
    assert!(construction.check_standard(helper).unwrap());
    row["resources"]["https://facts.test/helper"]["required"] = false.into();
    let replacement =
        Construction::new(build_environment(&row), Limits::default(), &|| false).unwrap();
    let changed = supplied_node(replacement.environment(), "https://facts.test/helper");
    assert!(!replacement.check_standard(changed).unwrap());
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
    let stop = Stop {
        checks: AtomicUsize::new(0),
        at: 0,
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
        let initial = &stop;
        let stop = Stop {
            checks: AtomicUsize::new(0),
            at,
        };
        let mut construction =
            Construction::new(environment.clone(), Limits::default(), initial).unwrap();
        construction.control = &stop;
        assert!(construction.check_standard(node).is_err(), "{at}");
    }
    let mut construction = Construction::new(environment, Limits::default(), &|| false).unwrap();
    construction.check_standard(node).unwrap();
    construction.control = &stop;
    assert!(construction.check_standard(node).is_err());
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
                let root = Plan::at(&construction, entry, PreparationLimits::default()).unwrap();
                let result = meta::validate(&root, &construction);
                if remaining == 8 {
                    result.unwrap();
                    construction.finish().unwrap();
                    // A second selected-meta walk hits completed programs even
                    // though no helper admission remains.
                    meta::validate(&root, &construction).unwrap();
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
