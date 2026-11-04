use super::normalized::Summary;
use super::*;
use crate::pattern::{self, Reading};
use openbindings_internal_schema::{FailureReason, preparation::Attempt};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! {
    static PARSER_ENTRIES: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn entered_parser() {
    PARSER_ENTRIES.with(|entries| entries.set(entries.get() + 1));
}

fn entries() -> usize {
    PARSER_ENTRIES.with(Cell::get)
}

fn reset_entries() {
    PARSER_ENTRIES.with(|entries| entries.set(0));
}

fn failure(error: Error) -> Failure {
    match error {
        Error::Attempt(failure) => failure,
        other => panic!("expected attempt failure: {other:?}"),
    }
}

#[derive(Default)]
struct Recorder {
    work: AtomicUsize,
    allocation: AtomicUsize,
}

impl Control for Recorder {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn check(&self) -> Result<(), Failure> {
        self.charge_work(1)
    }
    fn charge_work(&self, units: usize) -> Result<(), Failure> {
        self.work.fetch_add(units, Ordering::Relaxed);
        Ok(())
    }
    fn charge_allocation(&self, bytes: usize) -> Result<(), Failure> {
        self.check()?;
        self.allocation.fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn owns_one_complete_tree_after_guard_admission() {
    reset_entries();
    let form = r"((?-u:\b))[0-9A-Z\x{E000}]a{12,34}?";
    let parsed = prepare(form, 2, 32, &|| false).unwrap();
    assert_eq!(entries(), 1);
    assert_eq!(parsed.span().end.offset, form.len());
}

#[test]
fn grammar_depth_and_backend_representation_are_local_refusals() {
    reset_entries();
    assert!(matches!(
        prepare("(?i:a)", 32, 32, &|| false),
        Err(Error::Refused(Refusal::Normalized(
            normalized::Refusal::Grammar { .. }
        )))
    ));
    assert!(matches!(
        prepare("(a)", 0, 32, &|| false),
        Err(Error::Refused(Refusal::Normalized(
            normalized::Refusal::Depth { .. }
        )))
    ));
    assert_eq!(entries(), 0);
    for (form, depth) in [("a{4294967296}", 32), ("(((a)))", 0), ("*", 32)] {
        assert!(matches!(
            prepare(form, 32, depth, &|| false),
            Err(Error::Refused(Refusal::Representation))
        ));
    }
    assert_eq!(entries(), 3);
}

#[test]
fn exact_shared_allowances_and_one_below() {
    for form in ["", "a", "a0000|b0000", "[a-z]", "((a|b)*)"] {
        let recorder = Recorder::default();
        drop(prepare(form, 32, 32, &recorder).unwrap());
        let work = recorder.work.load(Ordering::Relaxed);
        let allocation = recorder.allocation.load(Ordering::Relaxed);
        reset_entries();
        let result = Attempt::run(work, allocation, &|| false, |attempt| {
            prepare(form, 32, 32, attempt)
        });
        drop(result.unwrap().unwrap());
        assert_eq!(entries(), 1, "{form}");

        // One missing work unit refuses completion after entering the parser.
        reset_entries();
        let result = Attempt::run(work - 1, allocation, &|| false, |attempt| {
            prepare(form, 32, 32, attempt)
        });
        assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
        assert_eq!(entries(), 1, "{form}");

        assert_eq!(
            allocation, 0,
            "dependency construction is not an SDK storage reservation"
        );
    }
}

#[test]
fn refused_precharges_never_enter_the_parser() {
    for limit in [0, 1, 40, 80] {
        reset_entries();
        let result = Attempt::run(limit, usize::MAX, &|| false, |attempt| {
            prepare("a0000|b0000", 32, 32, attempt)
        });
        assert_eq!(result.unwrap_err().code, "preparation-sdk-work-units");
        assert_eq!(entries(), 0);
    }
    reset_entries();
    let result = Attempt::run(usize::MAX, 0, &|| false, |attempt| {
        attempt.charge_allocation(1).unwrap_err();
        assert_eq!(
            failure(prepare("a", 32, 32, attempt).unwrap_err()).code,
            "preparation-sdk-reservation-bytes"
        );
    });
    assert_eq!(
        result.unwrap_err().code,
        "preparation-sdk-reservation-bytes"
    );
    assert_eq!(entries(), 0);
}

#[test]
fn cancellation_before_and_after_parsing_cannot_publish_ownership() {
    reset_entries();
    assert_eq!(
        failure(prepare("a", 32, 32, &|| true).unwrap_err()).reason,
        FailureReason::Cancelled
    );
    assert_eq!(entries(), 0);

    let result = Attempt::run(usize::MAX, usize::MAX, &|| entries() > 0, |attempt| {
        prepare("((a|b)*)", 32, 32, attempt)
    });
    assert_eq!(result.unwrap_err().reason, FailureReason::Cancelled);
    assert_eq!(entries(), 1);
    // A later call has independent control and owns a fresh successful AST.
    drop(prepare("((a|b)*)", 32, 32, &|| false).unwrap());
    assert_eq!(entries(), 2);
}

#[test]
fn structural_boundaries_refuse_before_parser_entry() {
    for (limit, class) in [(profile::AST_NODES, false), (profile::CLASS_ITEMS, true)] {
        for delta in [-1isize, 0, 1] {
            let n = limit.checked_add_signed(delta).unwrap();
            let form = if class {
                let items: String = (0..n)
                    .map(|i| {
                        let value = i * 2;
                        let scalar = if value >= 0xD800 {
                            value + 0x800
                        } else {
                            value
                        };
                        format!("\\x{{{scalar:X}}}")
                    })
                    .collect();
                format!("[{items}]")
            } else {
                "a".repeat(n - 2)
            };
            reset_entries();
            let result = prepare(&form, usize::MAX, u32::MAX, &|| false);
            if delta <= 0 {
                result.unwrap();
                assert_eq!(entries(), 1);
            } else {
                assert!(matches!(
                    result,
                    Err(Error::Refused(Refusal::ConstructionLimit))
                ));
                assert_eq!(entries(), 0);
            }
        }
    }
    let huge = Summary {
        literals: usize::MAX,
        ..Summary::default()
    };
    assert!(!profile::admits_normalized(&huge));
}

#[test]
fn shared_node_exhaustion_refuses_before_another_parser_entry() {
    let budget = profile::Budget::default();
    let full = "a".repeat(profile::AST_NODES - 2);
    reset_entries();
    for _ in 0..16 {
        drop(super::prepare(&full, 33, 250, &budget, &|| false).unwrap());
    }
    assert_eq!(entries(), 16);
    let failed = failure(super::prepare("a", 33, 250, &budget, &|| false).unwrap_err());
    assert_eq!(failed.code, "pattern-construction-ast-nodes");
    assert_eq!(entries(), 16);
    assert_eq!(budget.check(&|| false).unwrap_err().code, failed.code);
}

#[test]
fn unchanged_reader_forms_use_the_guarded_stage() {
    let mut linear = 0;
    for line in include_str!("../normalized/tests/reader-inputs.jsonl").lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let source = row["pattern"].as_str().unwrap();
        let Ok(Reading::Linear(reading)) = pattern::read(source, 32, 1024 * 1024) else {
            continue;
        };
        reset_entries();
        let result = prepare(&reading.form, 32, 250, &|| false);
        if matches!(row["id"].as_u64(), Some(1184 | 1185)) {
            assert!(matches!(
                result,
                Err(Error::Refused(Refusal::ConstructionLimit))
            ));
            assert_eq!(
                entries(),
                0,
                "preserved construction attack must not reach parser"
            );
        } else {
            assert_eq!(entries(), 1, "{}: {result:?}", row["id"]);
            result.unwrap();
        }
        linear += 1;
    }
    assert_eq!(linear, 1142);
}

fn prepare(form: &str, groups: usize, depth: u32, control: &dyn Control) -> Result<Ast, Error> {
    super::prepare(form, groups, depth, &profile::Budget::default(), control)
}

#[test]
fn bare_empty_alternations_use_their_actual_linear_node_bound() {
    for separators in [0, 1, 2, 10_000, 16_382] {
        let form = "|".repeat(separators);
        let summary = normalized::inspect(&form, 32, &|| false).unwrap();
        assert_eq!(profile::ast_nodes(&summary), Some(separators + 2));
        let parsed = prepare(&form, 32, 250, &|| false).unwrap();
        let expected = regex_syntax::ast::parse::Parser::new()
            .parse(&form)
            .unwrap();
        assert_eq!(parsed, expected);
        match &parsed {
            Ast::Empty(_) => assert_eq!(separators, 0),
            Ast::Alternation(node) => {
                assert_eq!(node.asts.len(), separators + 1);
                assert!(node.asts.iter().all(|child| matches!(child, Ast::Empty(_))));
            }
            other => panic!("unexpected bare-alternation AST: {other:?}"),
        }
    }
    reset_entries();
    assert!(matches!(
        prepare(&"|".repeat(16_383), 32, 250, &|| false),
        Err(Error::Refused(Refusal::ConstructionLimit))
    ));
    assert_eq!(entries(), 0);
    for source in [r"\|", "(|)", "a|", "^|", "[a]|", "a*|"] {
        let Reading::Linear(linear) = pattern::read(source, 32, 1 << 20).unwrap() else {
            panic!("supported exclusion control: {source}")
        };
        let summary = normalized::inspect(&linear.form, 32, &|| false).unwrap();
        assert!(
            profile::ast_nodes(&summary).unwrap() > summary.alternatives + 2,
            "{source}"
        );
    }
}
