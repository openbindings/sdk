//! Array selection, coherent contains counts and unevaluated-item guards.
use super::*;
pub(super) enum Cursor {
    Items {
        index: usize,
        waiting: bool,
    },
    Contains {
        index: usize,
        counts: counts::Matches,
        waiting: bool,
    },
    Unevaluated {
        index: usize,
        evaluated: Formula,
        waiting: bool,
    },
}
impl Cursor {
    pub(super) fn start() -> Self {
        Self::Items {
            index: 0,
            waiting: false,
        }
    }
}
pub(super) fn advance<'v>(
    frame: &mut Frame<'v>,
    run: &mut Run<'_>,
    cursor: Cursor,
    returned: Option<State>,
) -> Result<Action<'v>, NoVerdict> {
    if frame.value.kind() != JsonKind::Array {
        return Ok(Action::Complete);
    }
    let schema = frame.schema(run.plan);
    match cursor {
        Cursor::Items { mut index, waiting } => {
            if waiting {
                let result = returned.expect("item result");
                frame.state.valid = run.graph.and(frame.state.valid, result.valid)?;
                frame.state.inherit_failure(&result, TRUE);
                if run.plan.annotations.items {
                    run.graph.charge(96)?;
                    frame.state.items.insert(index, TRUE);
                }
                index += 1;
            }
            let tuple = schema.profile.edition() != Dialect::Draft202012
                && schema
                    .get("items")
                    .is_some_and(|v| v.kind() == JsonKind::Array);
            let prefix = schema.get(if tuple { "items" } else { "prefixItems" });
            while let Some(item) = frame.value.element(index) {
                run.graph.tick()?;
                if let Some(child) = prefix
                    .and_then(|prefix| prefix.element(index))
                    .or_else(|| schema.get(if tuple { "additionalItems" } else { "items" }))
                {
                    frame.phase = Phase::Array(Cursor::Items {
                        index,
                        waiting: true,
                    });
                    return frame.child(run, child, item);
                }
                index += 1;
            }
            frame.phase = Phase::Array(Cursor::Contains {
                index: 0,
                counts: counts::Matches::default(),
                waiting: false,
            });
        }
        Cursor::Contains {
            mut index,
            mut counts,
            waiting,
        } => {
            if waiting {
                let result = returned.expect("contains result");
                counts.add(result.valid, &mut run.graph)?;
                if run.plan.annotations.items && schema.profile.edition() == Dialect::Draft202012 {
                    let old = frame.state.items.get(&index).copied().unwrap_or(FALSE);
                    run.graph.charge(96)?;
                    frame
                        .state
                        .items
                        .insert(index, run.graph.or(old, result.valid)?);
                }
                index += 1;
            }
            if let Some(child) = schema.get("contains") {
                if let Some(item) = frame.value.element(index) {
                    frame.phase = Phase::Array(Cursor::Contains {
                        index,
                        counts,
                        waiting: true,
                    });
                    return frame.child(run, child, item);
                }
                let prepared = run.plan.operands.get(&frame.visit.schema);
                let mut count = |slot: usize, key: &str, default: u64| -> Result<u64, NoVerdict> {
                    let reading = prepared.and_then(|record| record.contains[slot].as_ref());
                    if reading.is_none() && schema.get(key).is_none() {
                        return Ok(default);
                    }
                    run.count_operand(&schema, key, reading)?.ok_or_else(|| {
                        no_verdict(
                            NoVerdictReason::UnsupportedCapability,
                            "contains-count-shape",
                            "a contains bound must be a nonnegative integer",
                        )
                    })
                };
                let minimum = count(0, "minContains", 1)?;
                let maximum = count(1, "maxContains", u64::MAX)?;
                let valid = counts.within(minimum, maximum, &mut run.graph)?;
                frame.state.valid = run.graph.and(frame.state.valid, valid)?;
                if valid == FALSE {
                    run.record_failure(
                        &mut frame.state,
                        frame.visit.schema,
                        frame.value,
                        Some("contains"),
                    )?;
                }
            }
            frame.phase = Phase::Array(Cursor::Unevaluated {
                index: 0,
                evaluated: FALSE,
                waiting: false,
            });
        }
        Cursor::Unevaluated {
            mut index,
            evaluated,
            waiting,
        } => {
            if waiting {
                let result = returned.expect("unevaluated item result");
                let valid = run.graph.or(evaluated, result.valid)?;
                frame.state.valid = run.graph.and(frame.state.valid, valid)?;
                if evaluated == FALSE {
                    frame.state.inherit_failure(&result, TRUE);
                }
                run.graph.charge(96)?;
                frame.state.items.insert(index, TRUE);
                index += 1;
            }
            if let Some(child) = schema.get("unevaluatedItems")
                && let Some(item) = frame.value.element(index)
            {
                let evaluated = frame.state.items.get(&index).copied().unwrap_or(FALSE);
                frame.phase = Phase::Array(Cursor::Unevaluated {
                    index,
                    evaluated,
                    waiting: true,
                });
                return frame.child(run, child, item);
            }
            return Ok(Action::Complete);
        }
    }
    Ok(Action::Continue)
}
