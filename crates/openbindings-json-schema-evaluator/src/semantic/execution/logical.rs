//! Same-instance logical applicators and their annotation guards.
use super::*;

pub(super) enum Cursor {
    Branches {
        operator: usize,
        index: usize,
        combined: State,
        one: Formula,
        zero: Formula,
        waiting: bool,
    },
    Not {
        waiting: bool,
    },
    Conditional {
        part: usize,
        condition: State,
        yes: State,
        waiting: bool,
    },
}
impl Cursor {
    pub(super) fn start() -> Self {
        Self::branches(0)
    }
    fn branches(operator: usize) -> Self {
        Self::Branches {
            operator,
            index: 0,
            combined: State::truth(if operator == 0 { TRUE } else { FALSE }),
            one: FALSE,
            zero: TRUE,
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
    let schema = frame.schema(run.plan);
    match cursor {
        Cursor::Branches {
            operator,
            mut index,
            mut combined,
            mut one,
            mut zero,
            waiting,
        } => {
            if waiting {
                let result = returned.expect("logical branch result");
                combined.valid = if operator == 0 {
                    run.graph.and(combined.valid, result.valid)?
                } else {
                    run.graph.or(combined.valid, result.valid)?
                };
                if operator == 0 {
                    combined.inherit_failure(&result, TRUE);
                }
                if operator == 2 {
                    let chosen = run.graph.and(zero, result.valid)?;
                    let no = run.graph.not(result.valid)?;
                    let prior = run.graph.and(one, no)?;
                    one = run.graph.or(chosen, prior)?;
                    zero = run.graph.and(zero, no)?;
                }
                run.annotations(&mut combined, &result, TRUE)?;
                index += 1;
            }
            let keyword = ["allOf", "anyOf", "oneOf"][operator];
            if let Some(branches) = schema.get(keyword).filter(|v| v.kind() == JsonKind::Array) {
                if let Some(child) = branches.element(index) {
                    frame.phase = Phase::Logical(Cursor::Branches {
                        operator,
                        index,
                        combined,
                        one,
                        zero,
                        waiting: true,
                    });
                    return frame.child(run, child, frame.value);
                }
                if operator == 2 {
                    combined.valid = one;
                }
                if operator != 0 && combined.valid == FALSE {
                    run.record_failure(
                        &mut combined,
                        frame.visit.schema,
                        frame.value,
                        Some(keyword),
                    )?;
                }
                run.conjunction(&mut frame.state, &combined)?;
            }
            frame.phase = Phase::Logical(if operator < 2 {
                Cursor::branches(operator + 1)
            } else {
                Cursor::Not { waiting: false }
            });
        }
        Cursor::Not { waiting } => {
            if waiting {
                let result = returned.expect("not result");
                let not = run.graph.not(result.valid)?;
                frame.state.valid = run.graph.and(frame.state.valid, not)?;
                if not == FALSE {
                    run.record_failure(
                        &mut frame.state,
                        frame.visit.schema,
                        frame.value,
                        Some("not"),
                    )?;
                }
            } else if let Some(child) = schema.get("not") {
                frame.phase = Phase::Logical(Cursor::Not { waiting: true });
                return frame.child(run, child, frame.value);
            }
            frame.phase = Phase::Logical(Cursor::Conditional {
                part: 0,
                condition: State::truth(TRUE),
                yes: State::truth(TRUE),
                waiting: false,
            });
        }
        Cursor::Conditional {
            mut part,
            mut condition,
            mut yes,
            waiting,
        } => {
            if waiting {
                let result = returned.expect("conditional child result");
                match part {
                    0 => condition = result,
                    1 => yes = result,
                    2 => {
                        finish(frame, run, &condition, &yes, &result)?;
                        return Ok(Action::Continue);
                    }
                    _ => unreachable!("conditional part"),
                }
                part += 1;
            }
            while part < 3 {
                if let Some(child) = schema.get(["if", "then", "else"][part]) {
                    frame.phase = Phase::Logical(Cursor::Conditional {
                        part,
                        condition,
                        yes,
                        waiting: true,
                    });
                    return frame.child(run, child, frame.value);
                }
                if part == 0 {
                    frame.phase = Phase::Object(object::Cursor::start());
                    return Ok(Action::Continue);
                }
                part += 1;
            }
            finish(frame, run, &condition, &yes, &State::truth(TRUE))?;
        }
    }
    Ok(Action::Continue)
}
fn finish(
    frame: &mut Frame<'_>,
    run: &mut Run<'_>,
    condition: &State,
    yes: &State,
    no: &State,
) -> Result<(), NoVerdict> {
    let selected = run.graph.choose(condition.valid, yes.valid, no.valid)?;
    frame.state.valid = run.graph.and(frame.state.valid, selected)?;
    if condition.valid == TRUE {
        frame.state.inherit_failure(yes, TRUE);
    } else if condition.valid == FALSE {
        frame.state.inherit_failure(no, TRUE);
    }
    run.annotations(&mut frame.state, condition, TRUE)?;
    run.annotations(&mut frame.state, yes, condition.valid)?;
    let otherwise = run.graph.not(condition.valid)?;
    run.annotations(&mut frame.state, no, otherwise)?;
    frame.phase = Phase::Object(object::Cursor::start());
    Ok(())
}
