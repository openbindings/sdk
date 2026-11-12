//! Iterative prospective HIR inventory, before any HIR smart constructor runs.
//!
//! One frame per AST ancestor folds scalar facts; wide alternatives never need
//! a vector of child summaries. The owner keeps structural pressure facts with its AST.
use openbindings_internal_schema::{Control, Failure};
use regex_syntax::ast::{Ast, ClassSet, ClassSetItem, GroupKind, RepetitionKind, RepetitionRange};
use std::mem::size_of;

mod arithmetic;
mod fold;
use arithmetic::{Amount, checked};
use fold::Frame;

#[derive(Debug)]
pub(super) struct Inventoried {
    pub tree: Ast,
    pub summary: Summary,
}

#[derive(Debug)]
pub(super) enum Error {
    Representation,
    ConstructionLimit,
    Attempt(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Root {
    Empty,
    Literal,
    Class,
    Unary,
    Look,
    Concat,
    Alternate,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    pub root: Root,
    pub first_branch_weight: usize,
    pub retained_nodes: usize,
    pub frontier: usize,
    pub exposed_ranges: usize,
    pub all_concat: bool,
    pub literal_bytes: usize,
    pub zero_width_only: bool,
    pub possible_nonempty: bool,
    pub height: usize,
}
impl Default for Shape {
    fn default() -> Self {
        Self {
            root: Root::Empty,
            first_branch_weight: 1,
            retained_nodes: 1,
            frontier: 1,
            exposed_ranges: 0,
            all_concat: false,
            literal_bytes: 0,
            zero_width_only: true,
            possible_nonempty: false,
            height: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Pressure {
    pub reconstruction_entries: usize,
    pub literal_byte_revisits: usize,
    pub candidate_range_slots: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub shape: Shape,
    pub pressure: Pressure,
    pub maximum_prefix_calls: usize,
    pub equality_height: usize,
}

// Only these descendant facts participate in a parent's construction formula.
// Additive whole-walk sites are collected once, outside the frame stack.
#[derive(Clone, Copy, Debug, Default)]
struct Subtree {
    shape: Shape,
    alternatives: usize,
    literal_bytes: usize,
    ranges: usize,
}

#[derive(Clone, Copy, Debug)]
enum InternalError {
    Representation,
    Overflow,
}
type Result<T> = std::result::Result<T, InternalError>;

fn leaf(tree: &Ast) -> Result<Subtree> {
    let mut result = Subtree::default();
    match tree {
        Ast::Empty(_) => {}
        Ast::Literal(l) => {
            result.literal_bytes = l.c.len_utf8();
            result.shape.root = Root::Literal;
            result.shape.literal_bytes = result.literal_bytes;
            result.shape.zero_width_only = false;
            result.shape.possible_nonempty = true;
        }
        Ast::Assertion(_) => result.shape.root = Root::Look,
        Ast::ClassBracketed(class) => {
            let items = match &class.kind {
                ClassSet::Item(ClassSetItem::Union(union)) => union.items.as_slice(),
                ClassSet::Item(item) => std::slice::from_ref(item),
                _ => return Err(InternalError::Representation),
            };
            // The prior closed grammar proves these are flat positive ranges;
            // inspect only a possible singleton here, never rescan a wide set.
            result.ranges = items.len();
            result.shape.root = Root::Class;
            result.shape.exposed_ranges = items.len();
            result.shape.zero_width_only = false;
            result.shape.possible_nonempty = !items.is_empty();
            let singleton = match items {
                [ClassSetItem::Literal(l)] => Some(l.c),
                [ClassSetItem::Range(r)] if r.start.c == r.end.c => Some(r.start.c),
                [_] | [] => None,
                _ => None,
            };
            if let Some(c) = singleton {
                result.literal_bytes = c.len_utf8();
                result.shape.literal_bytes = c.len_utf8();
                result.shape.root = Root::Literal;
                result.shape.exposed_ranges = 0;
            }
        }
        _ => return Err(InternalError::Representation),
    }
    Ok(result)
}

fn unary(tree: &Ast, child: Subtree) -> Result<Subtree> {
    let mut result = Subtree {
        alternatives: child.alternatives,
        literal_bytes: child.literal_bytes,
        ranges: child.ranges,
        ..Subtree::default()
    };
    let shape = child.shape;
    match tree {
        Ast::Group(group) => {
            if matches!(group.kind, GroupKind::CaptureIndex(_)) {
                result.shape.zero_width_only = shape.zero_width_only;
                result.shape.possible_nonempty = shape.possible_nonempty;
                result.shape.root = Root::Unary;
                result.shape.retained_nodes = checked(Amount::from(shape.retained_nodes) + 1)?;
                result.shape.height = checked(Amount::from(shape.height) + 1)?;
            } else {
                result.shape = shape;
            }
        }
        Ast::Repetition(repetition) => {
            let (minimum, maximum) = bounds(&repetition.op.kind);
            let (minimum, maximum) = if shape.zero_width_only {
                (minimum.min(1), Some(maximum.unwrap_or(1).min(1)))
            } else {
                (minimum, maximum)
            };
            result.shape.zero_width_only = shape.zero_width_only || maximum == Some(0);
            result.shape.possible_nonempty = shape.possible_nonempty && maximum != Some(0);
            if minimum == 0 && maximum == Some(0) {
            } else if minimum == 1 && maximum == Some(1) {
                result.shape = shape;
            } else {
                result.shape.root = if minimum == 0 || shape.possible_nonempty {
                    Root::Unary
                } else {
                    Root::Unknown
                };
                result.shape.retained_nodes = checked(Amount::from(shape.retained_nodes) + 1)?;
                result.shape.height = checked(Amount::from(shape.height) + 1)?;
                result.shape.frontier = result.shape.retained_nodes;
                result.shape.exposed_ranges =
                    checked(Amount::from(result.ranges) + result.shape.retained_nodes)?;
            }
        }
        _ => return Err(InternalError::Representation),
    }
    Ok(result)
}
fn bounds(kind: &RepetitionKind) -> (u32, Option<u32>) {
    match kind {
        RepetitionKind::ZeroOrOne => (0, Some(1)),
        RepetitionKind::ZeroOrMore => (0, None),
        RepetitionKind::OneOrMore => (1, None),
        RepetitionKind::Range(range) => match range {
            RepetitionRange::Exactly(n) => (*n, Some(*n)),
            RepetitionRange::AtLeast(n) => (*n, None),
            RepetitionRange::Bounded(a, b) => (*a, Some(*b)),
        },
    }
}

fn map_error(error: InternalError, control: &dyn Control) -> Error {
    if let Err(failure) = control.check() {
        return Error::Attempt(failure);
    }
    match error {
        InternalError::Representation => Error::Representation,
        InternalError::Overflow => Error::ConstructionLimit,
    }
}

/// Structural facts remain attached to the immutable AST they describe.
pub(super) fn inspect(tree: Ast, control: &dyn Control) -> std::result::Result<Inventoried, Error> {
    let summary = walk(&tree, control)?;
    control.check().map_err(Error::Attempt)?;
    Ok(Inventoried { tree, summary })
}

fn walk(root: &Ast, control: &dyn Control) -> std::result::Result<Summary, Error> {
    let scratch = openbindings_internal_schema::preparation::Scratch::new(control);
    let control: &dyn Control = &scratch;
    control.check().map_err(Error::Attempt)?;
    let mut totals = Summary::default();
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut next = Some(root);
    let mut completed = None;
    loop {
        // A node dispatch or one completed-child fold has bounded scalar work.
        // Input width appears only as repeated charged folds, never one batch.
        if next.is_none() && stack.is_empty() {
            let child: Subtree = completed.take().ok_or(Error::Representation)?;
            totals.shape = child.shape;
            return Ok(totals);
        }
        control.charge_work(1).map_err(Error::Attempt)?;
        control.check().map_err(Error::Attempt)?;
        if let Some(tree) = next.take() {
            if let Some(frame) = Frame::new(tree) {
                if stack.len() > super::profile::AST_DEPTH as usize {
                    return Err(map_error(InternalError::Overflow, control));
                }
                if stack.len() == stack.capacity() {
                    let capacity =
                        (stack.capacity().max(2) * 2).min(super::profile::AST_DEPTH as usize + 1);
                    control
                        .charge_allocation(capacity * size_of::<Frame<'_>>())
                        .map_err(Error::Attempt)?;
                    if stack.try_reserve_exact(capacity - stack.len()).is_err() {
                        return Err(map_error(InternalError::Overflow, control));
                    }
                }
                stack.push(frame);
                next = stack.last_mut().and_then(Frame::next);
                if next.is_none() {
                    completed = Some(
                        stack
                            .pop()
                            .unwrap()
                            .finish(&mut totals)
                            .map_err(|e| map_error(e, control))?,
                    );
                }
            } else {
                completed = Some(leaf(tree).map_err(|e| map_error(e, control))?);
            }
        } else {
            let child = completed.take().ok_or(Error::Representation)?;
            let Some(frame) = stack.last_mut() else {
                return Err(Error::Representation);
            };
            frame.accept(child).map_err(|e| map_error(e, control))?;
            next = frame.next();
            if next.is_none() {
                completed = Some(
                    stack
                        .pop()
                        .unwrap()
                        .finish(&mut totals)
                        .map_err(|e| map_error(e, control))?,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
