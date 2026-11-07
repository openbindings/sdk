//! Streaming structural scores for the locked forward capture-free compiler.
//! These quantities admit helper input and joins; they are not CPU/byte quotas.
use super::{Error, MAXIMUM_PATH, compiler_path};
use openbindings_internal_schema::Control;
use regex_syntax::{
    hir::{Class, Hir, HirKind, Look, Repetition},
    utf8::Utf8Sequences,
};
use std::mem::size_of;

const SENTINEL: usize = 1_048_577;
// One lazy, reused iterator: at most 11 pending scalar ranges, capacity 16.
// The pinned growth path requests at most (1+2+4+8+16)*8 = 248 bytes.
const UTF8_SCRATCH_BYTES: usize = 512;
fn add(a: usize, b: usize) -> usize {
    a.saturating_add(b).min(SENTINEL)
}
fn multiply(a: usize, b: usize) -> usize {
    a.saturating_mul(b).min(SENTINEL)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Scores {
    pub dispatch: usize,
    pub helper_payload: usize,
    pub empty_tail_pressure: usize,
}
#[derive(Clone, Copy, Debug)]
struct Tail {
    prefix: usize,
    suffix: usize,
    maximum: usize,
    pure: bool,
    unions: usize,
}
impl Tail {
    fn empty() -> Self {
        Self {
            prefix: 1,
            suffix: 1,
            maximum: 1,
            pure: true,
            unions: 0,
        }
    }
    fn terminal() -> Self {
        Self {
            prefix: 0,
            suffix: 0,
            maximum: 0,
            pure: false,
            unions: 0,
        }
    }
    fn concatenate(self, next: Self) -> Self {
        Self {
            prefix: add(self.prefix, if self.pure { next.prefix } else { 0 }),
            suffix: add(next.suffix, if next.pure { self.suffix } else { 0 }),
            maximum: self
                .maximum
                .max(next.maximum)
                .max(add(self.suffix, next.prefix)),
            pure: self.pure && next.pure,
            unions: add(self.unions, next.unions),
        }
    }
    fn times(self, count: usize) -> Self {
        match count {
            0 => Self::empty(),
            1 => self,
            _ => Self {
                prefix: if self.pure {
                    multiply(count, self.prefix)
                } else {
                    self.prefix
                },
                suffix: if self.pure {
                    multiply(count, self.suffix)
                } else {
                    self.suffix
                },
                maximum: self
                    .maximum
                    .max(add(self.suffix, self.prefix))
                    .max(if self.pure {
                        multiply(count, self.prefix)
                    } else {
                        0
                    }),
                pure: self.pure,
                unions: multiply(count, self.unions),
            },
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct Facts {
    dispatch: usize,
    helper: usize,
    tail: Tail,
}
impl Facts {
    fn leaf(helper: usize, tail: Tail) -> Self {
        Self {
            dispatch: 1,
            helper: helper.min(SENTINEL),
            tail,
        }
    }
    fn repeat(self, rep: &Repetition) -> Self {
        let minimum = rep.min as usize;
        let copies = rep.max.map_or(minimum.max(1), |n| n as usize);
        let child = self.tail;
        let tail = match rep.max {
            Some(maximum) if maximum == rep.min => child.times(minimum),
            Some(1) if minimum == 0 => Tail {
                prefix: 0,
                suffix: add(child.suffix, 1),
                maximum: child.maximum.max(add(child.suffix, 1)),
                pure: false,
                unions: add(child.unions, 2),
            },
            Some(maximum) => {
                let prefix = child.times(minimum);
                Tail {
                    prefix: prefix.prefix,
                    suffix: add(child.suffix, 1),
                    maximum: prefix.maximum.max(child.maximum).max(add(child.suffix, 1)),
                    pure: false,
                    unions: add(
                        multiply(maximum as usize, child.unions),
                        multiply(2, maximum as usize - minimum),
                    ),
                }
            }
            None if minimum == 0 && rep.sub.properties().minimum_len().is_some_and(|n| n > 0) => {
                Tail {
                    prefix: 0,
                    suffix: 0,
                    maximum: child.maximum,
                    pure: false,
                    unions: add(child.unions, 2),
                }
            }
            None if minimum == 0 => Tail {
                prefix: 0,
                suffix: 1,
                maximum: child.maximum.max(1),
                pure: false,
                unions: add(child.unions, 4),
            },
            None => {
                let mut tail = child.times(minimum);
                tail.suffix = 0;
                tail.pure = false;
                tail.unions = add(tail.unions, 2);
                tail
            }
        };
        Self {
            dispatch: add(1, multiply(copies, self.dispatch)),
            helper: multiply(copies, self.helper),
            tail,
        }
    }
}

struct Frame<'a> {
    node: &'a Hir,
    next: usize,
    facts: Facts,
}
impl<'a> Frame<'a> {
    fn child(&mut self) -> Option<&'a Hir> {
        let child = self.node.kind().subs().get(self.next)?;
        self.next += 1;
        Some(child)
    }
    fn accept(&mut self, child: Facts) {
        match self.node.kind() {
            HirKind::Capture(_) | HirKind::Repetition(_) => self.facts = child,
            HirKind::Concat(_) => {
                self.facts.dispatch = add(self.facts.dispatch, child.dispatch);
                self.facts.helper = add(self.facts.helper, child.helper);
                self.facts.tail = if self.next == 1 {
                    child.tail
                } else {
                    self.facts.tail.concatenate(child.tail)
                };
            }
            HirKind::Alternation(_) => {
                self.facts.dispatch = add(self.facts.dispatch, child.dispatch);
                self.facts.helper = add(self.facts.helper, child.helper);
                self.facts.tail.suffix = self.facts.tail.suffix.max(add(child.tail.suffix, 1));
                self.facts.tail.maximum = self
                    .facts
                    .tail
                    .maximum
                    .max(child.tail.maximum)
                    .max(add(child.tail.suffix, 1));
                self.facts.tail.unions = add(self.facts.tail.unions, child.tail.unions);
            }
            _ => unreachable!("only composite HIR has a frame"),
        }
    }
    fn finish(self) -> Facts {
        match self.node.kind() {
            HirKind::Capture(_) => Facts {
                dispatch: add(1, self.facts.dispatch),
                ..self.facts
            },
            HirKind::Repetition(rep) => self.facts.repeat(rep),
            _ => self.facts,
        }
    }
}

fn leaf(
    node: &Hir,
    utf8: &mut Option<Utf8Sequences>,
    control: &dyn Control,
) -> Result<Option<Facts>, Error> {
    let terminal = Tail::terminal();
    Ok(Some(match node.kind() {
        HirKind::Empty => Facts::leaf(0, Tail::empty()),
        HirKind::Literal(literal) => Facts::leaf(
            literal.0.len(),
            if literal.0.is_empty() {
                Tail::empty()
            } else {
                terminal
            },
        ),
        HirKind::Look(Look::Start | Look::End) => Facts::leaf(0, terminal),
        HirKind::Look(Look::WordAscii) => Facts::leaf(256, terminal),
        HirKind::Look(_) => {
            control.check()?;
            return Err(Error::Representation);
        }
        HirKind::Class(class) => {
            let payload = match class {
                Class::Bytes(class) => class.ranges().len(),
                Class::Unicode(class) if class.is_ascii() => class.ranges().len(),
                Class::Unicode(class) => {
                    let mut payload = class.ranges().len().min(SENTINEL);
                    for range in class.ranges() {
                        control.charge_work(1)?;
                        if let Some(iterator) = utf8.as_mut() {
                            iterator.reset(range.start(), range.end());
                        } else {
                            control.charge_allocation(UTF8_SCRATCH_BYTES)?;
                            *utf8 = Some(Utf8Sequences::new(range.start(), range.end()));
                        }
                        let iterator = utf8.as_mut().expect("lazy iterator initialized");
                        loop {
                            control.charge_work(1)?;
                            let Some(sequence) = iterator.next() else {
                                break;
                            };
                            payload = add(payload, 1 + sequence.as_slice().len());
                        }
                    }
                    payload
                }
            };
            Facts::leaf(
                payload,
                Tail {
                    suffix: 1,
                    maximum: 1,
                    ..terminal
                },
            )
        }
        HirKind::Alternation(children) => {
            control.charge_work(children.len())?;
            if children.len() < 2
                || !children
                    .iter()
                    .all(|node| matches!(node.kind(), HirKind::Literal(_)))
            {
                return Ok(None);
            }
            let mut bytes = 0;
            for child in children {
                control.charge_work(1)?;
                let HirKind::Literal(literal) = child.kind() else {
                    unreachable!()
                };
                bytes = add(bytes, literal.0.len());
            }
            Facts::leaf(
                add(children.len(), bytes),
                Tail {
                    prefix: 1,
                    suffix: 1,
                    maximum: 2,
                    pure: false,
                    unions: add(bytes, multiply(2, children.len())),
                },
            )
        }
        _ => return Ok(None),
    }))
}

/// Inspect once with ancestor-only scratch. A saturated child is not a refusal:
/// zero copies can annihilate it, so the caller checks only the completed root.
pub(super) fn inspect(hir: &Hir, control: &dyn Control) -> Result<Scores, Error> {
    compiler_path(hir, control)?;
    let mut utf8 = None;
    let mut ancestors: Vec<Frame<'_>> = Vec::new();
    let mut next = Some(hir);
    let mut completed = None;
    loop {
        control.charge_work(1)?;
        if let Some(node) = next.take() {
            if let Some(facts) = leaf(node, &mut utf8, control)? {
                completed = Some(facts);
            } else {
                if ancestors.len() >= MAXIMUM_PATH {
                    control.check()?;
                    return Err(Error::ConstructionLimit);
                }
                if ancestors.len() == ancestors.capacity() {
                    let capacity = (ancestors.capacity().max(2) * 2).min(MAXIMUM_PATH);
                    control.charge_allocation(capacity * size_of::<Frame<'_>>())?;
                    if ancestors
                        .try_reserve_exact(capacity - ancestors.len())
                        .is_err()
                    {
                        control.check()?;
                        return Err(Error::Size);
                    }
                }
                let arity = node.kind().subs().len().min(SENTINEL);
                let tail = if matches!(node.kind(), HirKind::Alternation(_)) {
                    Tail {
                        prefix: 0,
                        suffix: 1,
                        maximum: 1,
                        pure: false,
                        unions: arity,
                    }
                } else {
                    Tail::empty()
                };
                ancestors.push(Frame {
                    node,
                    next: 0,
                    facts: Facts::leaf(arity, tail),
                });
                next = ancestors.last_mut().and_then(Frame::child);
                if next.is_some() {
                    continue;
                }
                completed = Some(ancestors.pop().unwrap().finish());
            }
        }
        let facts = completed.take().expect("a completed subtree");
        let Some(parent) = ancestors.last_mut() else {
            let scores = Scores {
                dispatch: add(1, facts.dispatch),
                helper_payload: add(1, facts.helper),
                empty_tail_pressure: multiply(
                    add(facts.tail.unions, 2),
                    facts.tail.maximum.max(add(facts.tail.prefix, 1)),
                ),
            };
            drop(ancestors);
            drop(utf8);
            control.check()?;
            return Ok(scores);
        };
        parent.accept(facts);
        next = parent.child();
        if next.is_none() {
            completed = Some(ancestors.pop().unwrap().finish());
        }
    }
}

#[cfg(test)]
mod tests;
