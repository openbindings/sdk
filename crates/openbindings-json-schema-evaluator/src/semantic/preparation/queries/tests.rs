use super::*;
use openbindings_internal_schema::preparation::Attempt;
use std::sync::atomic::AtomicBool;

fn original(row: &serde_json::Value) -> Plan {
    use openbindings_internal_schema::preparation::EnvironmentRequest;
    let mut resources: Vec<_> = row["resources"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(uri, text)| ExternalSchema {
            uri: uri.clone(),
            document: JsonValue::parse(text.as_str().unwrap()).unwrap(),
        })
        .collect();
    let mut schema = row["schema"].as_str().unwrap();
    if row["external"].as_bool().unwrap() {
        resources.push(ExternalSchema {
            uri: "https://benchmark.invalid/root".into(),
            document: JsonValue::parse(schema).unwrap(),
        });
        schema = r#"{"$ref":"https://benchmark.invalid/root"}"#;
    }
    let document = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{schema}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let resources = ExternalSchemas::new(resources).unwrap();
    let schema = document.schema("entry").unwrap();
    let request = EvaluationRequest {
        schema: &schema,
        external_schemas: &resources,
    };
    let preparation = PreparationLimits::default();
    Attempt::run(
        preparation.max_sdk_work_units,
        preparation.max_sdk_reservation_bytes,
        &|| false,
        |attempt| {
            let environment = request.environment_with_control(
                preparation.max_schema_nodes,
                preparation.max_identity_bytes,
                attempt,
            )?;
            Plan::new(environment, Limits::default(), preparation, attempt)
        },
    )
    .unwrap()
    .unwrap()
}

#[test]
fn independent_reference_context_and_keyword_order_controls() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("tests/controls.json")).unwrap();
    assert_eq!(rows.len(), 19);
    for row in rows {
        let plan = original(&row);
        let value = JsonValue::parse(row["value"].as_str().unwrap()).unwrap();
        for detail in [Detail::Outcome, Detail::Explain] {
            let outcome = match plan.evaluate(&value, detail, &AtomicBool::new(false)) {
                ValueOutcome::Satisfies => "S",
                ValueOutcome::Fails { .. } => "F",
                ValueOutcome::Undefined { .. } => "U",
                ValueOutcome::NoVerdict { .. } => "N",
            };
            assert_eq!(outcome, row["required"].as_str().unwrap(), "{}", row["id"]);
        }
    }
}

#[test]
fn frozen_repeated_long_references_fit_unchanged_preparation_allowances() {
    for (count, length) in [(1000, 256), (5000, 1024)] {
        let uri = format!("https://memory.test/{}", "a".repeat(length));
        let reference = format!(r#"{{"$ref":"{uri}#/$defs/value"}}"#);
        let schema = format!(r#"{{"allOf":[{}]}}"#, vec![reference; count].join(","));
        let target = format!(r#"{{"$id":"{uri}","$defs":{{"value":{{"type":"integer"}}}}}}"#);
        let row = serde_json::json!({"schema":schema,"external":true,"resources":{uri:target}});
        let plan = original(&row);
        for (value, satisfies) in [("1", true), (r#""x""#, false)] {
            let value = JsonValue::parse(value).unwrap();
            for detail in [Detail::Outcome, Detail::Explain] {
                let outcome = plan.evaluate(&value, detail, &AtomicBool::new(false));
                assert!(
                    if satisfies {
                        matches!(outcome, ValueOutcome::Satisfies)
                    } else {
                        matches!(outcome, ValueOutcome::Fails { .. })
                    },
                    "{count}: {outcome:?}"
                );
            }
        }
    }
}

fn environment(schema: &str) -> SchemaEnvironment {
    let document = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{schema}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    document
        .schema("entry")
        .unwrap()
        .environment(
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
}

#[test]
fn saturated_cache_keeps_hits_and_resolves_new_keys_without_retaining_them() {
    let environment = environment("true");
    let space = environment.space();
    let references: Vec<_> = (0..MAX_KEYS + 2)
        .map(|i| format!("https://review.invalid/missing/{i}"))
        .collect();
    let mut queries = Queries::new(space);
    let first = queries
        .query(environment.entry(), &references[0], &|| false)
        .unwrap();
    for reference in &references[1..] {
        let query = queries
            .query(environment.entry(), reference, &|| false)
            .unwrap();
        assert_eq!(
            query.as_ref().as_ref().unwrap_err(),
            &space.query(environment.entry(), reference).unwrap_err()
        );
    }
    assert_eq!(queries.entries.len(), MAX_KEYS);
    let hit = queries
        .query(environment.entry(), &references[0], &|| false)
        .unwrap();
    assert!(Arc::ptr_eq(&first, &hit));
    let miss = queries
        .query(environment.entry(), &references[MAX_KEYS], &|| false)
        .unwrap();
    let again = queries
        .query(environment.entry(), &references[MAX_KEYS], &|| false)
        .unwrap();
    assert!(!Arc::ptr_eq(&miss, &again));
    assert_eq!(
        miss.as_ref().as_ref().unwrap_err(),
        again.as_ref().as_ref().unwrap_err()
    );
    drop(queries);
    assert_eq!(Arc::strong_count(&first), 2);
}

#[test]
fn storage_overflow_and_refused_backing_are_sticky_before_allocation() {
    for needed in [1, usize::MAX] {
        let mut values = Vec::<u64>::new();
        let failure = Attempt::run(100, 0, &|| false, |attempt| {
            assert!(
                crate::semantic::storage::reserve(&mut values, needed, &mut Preparation(attempt))
                    .is_err()
            );
            assert!(attempt.check().is_err());
            true
        })
        .unwrap_err();
        assert_eq!(failure.code, "preparation-sdk-reservation-bytes");
        assert_eq!(values.capacity(), 0);
    }
}

#[test]
fn hit_growth_and_cap_fallback_observe_exact_budgets_and_cancellation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Stop {
        checks: AtomicUsize,
        at: usize,
    }
    impl Control for Stop {
        fn is_cancelled(&self) -> bool {
            self.checks.fetch_add(1, Ordering::Relaxed) >= self.at
        }
    }
    let environment = environment("true");
    let references: Vec<_> = (0..MAX_KEYS + 2)
        .map(|i| format!("https://review.invalid/shared-prefix/{i}"))
        .collect();
    for initial in [0, 4, MAX_KEYS] {
        let run = |work, bytes, stop: &dyn Control| {
            let mut queries = Queries::new(environment.space());
            for reference in &references[..initial] {
                queries
                    .query(environment.entry(), reference, &|| false)
                    .unwrap();
            }
            Attempt::run(work, bytes, stop, |a| {
                let result = (|| {
                    queries.query(environment.entry(), &references[0], a)?;
                    queries.query(environment.entry(), &references[initial + 1], a)?;
                    Ok::<_, PreparationFailure>(())
                })();
                (result, a.allocated())
            })
        };
        let stop = Stop {
            checks: AtomicUsize::new(0),
            at: usize::MAX,
        };
        let (result, bytes) = run(usize::MAX, usize::MAX, &stop).unwrap();
        result.unwrap();
        let checks = stop.checks.load(Ordering::Relaxed);
        for at in 0..checks {
            assert_eq!(
                run(
                    usize::MAX,
                    usize::MAX,
                    &Stop {
                        checks: AtomicUsize::new(0),
                        at
                    }
                )
                .unwrap_err()
                .reason,
                openbindings_internal_schema::FailureReason::Cancelled,
                "{initial}/{at}"
            );
        }
        run(usize::MAX, bytes, &|| false).unwrap().0.unwrap();
        assert_eq!(
            run(usize::MAX, bytes - 1, &|| false).unwrap_err().code,
            "preparation-sdk-reservation-bytes"
        );
        let (mut lo, mut hi) = (0, 100_000);
        assert!(run(hi, bytes, &|| false).is_ok());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if run(mid, bytes, &|| false).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        run(lo, bytes, &|| false).unwrap().0.unwrap();
        assert_eq!(
            run(lo - 1, bytes, &|| false).unwrap_err().code,
            "preparation-sdk-work-units"
        );
    }
}

#[test]
fn repeated_results_keep_original_occurrences_and_full_miss_reservations() {
    let environment = environment(
        r#"{"allOf":[{"$ref":"https://review.invalid/missing"},{"$ref":"https://review.invalid/missing"}]}"#,
    );
    let construction =
        Construction::new(environment.clone(), Limits::default(), &|| false).unwrap();
    let checked = check_from(&construction, environment.entry()).unwrap();
    let queries: Vec<_> = checked.queries.iter().collect();
    assert_eq!(queries.len(), 2);
    assert_ne!(queries[0].0.0, queries[1].0.0);
    assert!(Arc::ptr_eq(queries[0].1, queries[1].1));

    let long = format!("https://review.invalid/{}", "x".repeat(10_000));
    let mut shared = Queries::new(environment.space());
    let first = shared.query(environment.entry(), &long, &|| false).unwrap();
    let hit = Attempt::run(100_000, 0, &|| false, |a| {
        shared.query(environment.entry(), &long, a)
    })
    .unwrap()
    .unwrap();
    assert!(Arc::ptr_eq(&first, &hit));
    let failure = Attempt::run(usize::MAX, 0, &|| false, |a| {
        Queries::new(environment.space()).query(environment.entry(), &long, a)
    })
    .unwrap_err();
    assert_eq!(failure.code, "preparation-sdk-reservation-bytes");
}
