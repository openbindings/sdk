//! Object applicators operate on one original member occurrence at a time.
use super::*;

pub(super) enum Cursor {
    Dependencies {
        next: usize,
        waiting: bool,
    },
    Member(Member),
    Unevaluated {
        index: usize,
        name: Option<Vec<u16>>,
        evaluated: Formula,
    },
}
pub(super) struct Member {
    index: usize,
    name: Option<Vec<u16>>,
    selected: Formula,
    phase: MemberPhase,
}
enum MemberPhase {
    Start,
    PropertyReturned,
    Pattern(usize),
    PatternReturned { next: usize, matched: Formula },
    Additional,
    AdditionalReturned,
    Name,
    NameReturned,
    Finish,
}
impl Cursor {
    pub(super) fn start() -> Self {
        Self::Dependencies {
            next: 0,
            waiting: false,
        }
    }
    fn member(index: usize) -> Self {
        Self::Member(Member {
            index,
            name: None,
            selected: FALSE,
            phase: MemberPhase::Start,
        })
    }
}
pub(super) fn advance<'v>(
    frame: &mut Frame<'v>,
    run: &mut Run<'_>,
    cursor: Cursor,
    returned: Option<State>,
) -> Result<Action<'v>, NoVerdict> {
    if frame.value.kind() != JsonKind::Object {
        frame.phase = Phase::Array(array::Cursor::start());
        return Ok(Action::Continue);
    }
    let schema = frame.schema(run.plan);
    match cursor {
        Cursor::Dependencies { mut next, waiting } => {
            if waiting {
                run.conjunction(
                    &mut frame.state,
                    &returned.expect("dependent schema result"),
                )?;
            }
            if let Some(dependencies) = schema
                .get("dependentSchemas")
                .or_else(|| schema.get("dependencies"))
            {
                while let Some(dependency) = backend::member_at(dependencies, next) {
                    run.graph.tick()?;
                    next += 1;
                    if dependency.value.kind() != JsonKind::Array
                        && run.named(frame.value, dependency.name)?.is_some()
                    {
                        frame.phase = Phase::Object(Cursor::Dependencies {
                            next,
                            waiting: true,
                        });
                        return frame.child(run, dependency.value, frame.value);
                    }
                }
            }
            frame.phase = Phase::Object(Cursor::member(0));
        }
        Cursor::Member(mut cursor) => {
            let Some(member) = backend::member_at(frame.value, cursor.index) else {
                frame.phase = Phase::Object(Cursor::Unevaluated {
                    index: 0,
                    name: None,
                    evaluated: FALSE,
                });
                return Ok(Action::Continue);
            };
            match cursor.phase {
                MemberPhase::Start => {
                    run.graph.tick()?;
                    if run.plan.annotations.properties {
                        cursor.name = Some(run.units(member.name)?);
                    }
                    if let Some(child) = schema
                        .get("properties")
                        .map(|p| run.named(p, member.name))
                        .transpose()?
                        .flatten()
                    {
                        cursor.phase = MemberPhase::PropertyReturned;
                        frame.phase = Phase::Object(Cursor::Member(cursor));
                        return frame.child(run, child, member.value);
                    }
                    cursor.phase = MemberPhase::Pattern(0);
                }
                MemberPhase::PropertyReturned => {
                    let child = returned.expect("property result");
                    frame.state.valid = run.graph.and(frame.state.valid, child.valid)?;
                    cursor.selected = TRUE;
                    insert_name(frame, run, cursor.name.as_deref(), TRUE)?;
                    cursor.phase = MemberPhase::Pattern(0);
                }
                MemberPhase::Pattern(index) => {
                    if let Some(pattern) = schema
                        .get("patternProperties")
                        .and_then(|p| backend::member_at(p, index))
                    {
                        run.graph
                            .charge(pattern.name.text().len().saturating_add(32))?;
                        let keyword = format!("patternProperties/{}", pattern.name.text());
                        let matched = run.pattern(
                            frame.visit.schema,
                            &keyword,
                            pattern.name,
                            member.name,
                            &frame.visit.context,
                        )?;
                        cursor.phase = MemberPhase::PatternReturned {
                            next: index + 1,
                            matched,
                        };
                        frame.phase = Phase::Object(Cursor::Member(cursor));
                        return frame.child(run, pattern.value, member.value);
                    }
                    cursor.phase = MemberPhase::Additional;
                }
                MemberPhase::PatternReturned { next, matched } => {
                    let child = returned.expect("pattern property result");
                    let applies = run.graph.implies(matched, child.valid)?;
                    frame.state.valid = run.graph.and(frame.state.valid, applies)?;
                    cursor.selected = run.graph.or(cursor.selected, matched)?;
                    if let Some(name) = &cursor.name {
                        let old = frame.state.properties.get(name).copied().unwrap_or(FALSE);
                        let combined = run.graph.or(old, matched)?;
                        insert_name(frame, run, Some(name), combined)?;
                    }
                    cursor.phase = MemberPhase::Pattern(next);
                }
                MemberPhase::Additional => {
                    if let Some(child) = schema.get("additionalProperties") {
                        cursor.phase = MemberPhase::AdditionalReturned;
                        frame.phase = Phase::Object(Cursor::Member(cursor));
                        return frame.child(run, child, member.value);
                    }
                    cursor.phase = MemberPhase::Name;
                }
                MemberPhase::AdditionalReturned => {
                    let child = returned.expect("additional property result");
                    let applies = run.graph.or(cursor.selected, child.valid)?;
                    frame.state.valid = run.graph.and(frame.state.valid, applies)?;
                    insert_name(frame, run, cursor.name.as_deref(), TRUE)?;
                    cursor.phase = MemberPhase::Name;
                }
                MemberPhase::Name => {
                    if let Some(child) = schema.get("propertyNames") {
                        cursor.phase = MemberPhase::NameReturned;
                        frame.phase = Phase::Object(Cursor::Member(cursor));
                        return frame.child(run, child, member.name);
                    }
                    cursor.phase = MemberPhase::Finish;
                }
                MemberPhase::NameReturned => {
                    frame.state.valid = run.graph.and(
                        frame.state.valid,
                        returned.expect("property name result").valid,
                    )?;
                    cursor.phase = MemberPhase::Finish;
                }
                MemberPhase::Finish => {
                    if let Some(name) = &cursor.name {
                        run.graph.charge(96 + name.len() * 2)?;
                    }
                    frame.phase = Phase::Object(Cursor::member(cursor.index + 1));
                    return Ok(Action::Continue);
                }
            }
            frame.phase = Phase::Object(Cursor::Member(cursor));
        }
        Cursor::Unevaluated {
            mut index,
            name,
            evaluated,
        } => {
            if let Some(name) = name {
                let result = returned.expect("unevaluated property result");
                let valid = run.graph.or(evaluated, result.valid)?;
                frame.state.valid = run.graph.and(frame.state.valid, valid)?;
                run.graph.charge(96)?;
                frame.state.properties.insert(name, TRUE);
                index += 1;
            }
            if let Some(child) = schema.get("unevaluatedProperties")
                && let Some(member) = backend::member_at(frame.value, index)
            {
                let name = run.units(member.name)?;
                let evaluated = frame.state.properties.get(&name).copied().unwrap_or(FALSE);
                frame.phase = Phase::Object(Cursor::Unevaluated {
                    index,
                    name: Some(name),
                    evaluated,
                });
                return frame.child(run, child, member.value);
            }
            frame.phase = Phase::Array(array::Cursor::start());
        }
    }
    Ok(Action::Continue)
}
fn insert_name(
    frame: &mut Frame<'_>,
    run: &mut Run<'_>,
    name: Option<&[u16]>,
    valid: Formula,
) -> Result<(), NoVerdict> {
    if let Some(name) = name {
        run.graph
            .charge(name.len().saturating_mul(2).saturating_add(256))?;
        frame.state.properties.insert(name.to_vec(), valid);
    }
    Ok(())
}
