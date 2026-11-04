//! Iterative prospective HIR inventory, before any HIR smart constructor runs.
//!
//! One frame per AST ancestor folds scalar facts; wide alternatives never need
//! a vector of child summaries. The owner keeps the certificate with its AST.
use super::ast::Parsed;
use openbindings_internal_schema::{Control, Failure, FailureReason};
use regex_syntax::ast::{Ast, ClassSet, ClassSetItem, GroupKind, RepetitionKind, RepetitionRange};
use std::mem::size_of;

mod arithmetic;
mod fold;
use arithmetic::{Amount, add, checked};
use fold::Frame;

#[derive(Debug)]
pub(super) struct Inventoried {
    pub parsed: Parsed,
    pub summary: Summary,
    pub work: usize,
    pub allocation: usize,
}

#[derive(Debug)]
pub(super) enum Error {
    Representation,
    TargetLayout,
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
pub(super) struct Work {
    pub concat_input: usize,
    pub concat_visit: usize,
    pub concat_bytes: usize,
    pub alternate_input: usize,
    pub alternate_frontier: usize,
    pub prefix_nodes: usize,
    pub prefix_bytes: usize,
    pub prefix_ranges: usize,
    pub prefix_split: usize,
    pub sort_pairs: usize,
    pub range_slots: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Summary {
    pub shape: Shape,
    pub work: Work,
    pub nodes: usize,
    pub alternatives: usize,
    pub unary: usize,
    pub literal_bytes: usize,
    pub ranges: usize,
    pub classes: usize,
    pub smart_allocation: usize,
    pub prefix_calls: usize,
    pub concat_allocation: usize,
    pub alternate_allocation: usize,
    pub maximum_prefix_calls: usize,
    pub equality_height: usize,
}
impl Default for Summary {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            work: Work::default(),
            nodes: 1,
            alternatives: 0,
            unary: 0,
            literal_bytes: 0,
            ranges: 0,
            classes: 0,
            smart_allocation: 0,
            prefix_calls: 0,
            concat_allocation: 0,
            alternate_allocation: 0,
            maximum_prefix_calls: 0,
            equality_height: 0,
        }
    }
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

impl Summary {
    pub(super) fn work_sites(&self, word: usize) -> Option<[usize; 21]> {
        let hir = match word {
            8 => 48usize,
            4 => 28,
            _ => return None,
        }
        .div_ceil(word);
        let range = 8usize.div_ceil(word);
        let n = Amount::from(self.nodes);
        let r = Amount::from(self.ranges);
        let u = Amount::from(self.unary);
        let b = Amount::from(self.literal_bytes);
        let p = Amount::from(self.prefix_calls);
        let w = &self.work;
        let terms = [
            (n + r) * 4,
            (n * 2 + r) * 4 * hir,
            (n + u * 2) * 16,
            b * 8,
            r * 8,
            Amount::from(w.concat_input) * 4 * hir,
            Amount::from(w.concat_visit) * 17,
            Amount::from(w.concat_visit) * 3 * hir,
            Amount::from(w.concat_visit) * 22,
            Amount::from(w.concat_bytes) * 6,
            Amount::from(w.alternate_input) * 4 * hir,
            Amount::from(w.alternate_frontier) * 33,
            Amount::from(w.alternate_frontier) * 3 * hir,
            Amount::from(w.alternate_frontier) * 9,
            Amount::from(w.prefix_nodes) * 22,
            Amount::from(w.prefix_bytes),
            Amount::from(w.prefix_ranges) * 2,
            Amount::from(w.prefix_split) * 3 * hir,
            Amount::from(w.sort_pairs) * 2,
            Amount::from(w.range_slots) * range,
            (n + p) * 3 * hir + (n * 2 + p) * 16,
        ];
        let mut sites = [0; 21];
        for (site, term) in sites.iter_mut().zip(terms) {
            *site = term.0?;
        }
        Some(sites)
    }

    pub(super) fn allocation_sites(&self) -> Option<[usize; 7]> {
        let n = Amount::from(self.nodes);
        let u = Amount::from(self.unary);
        let b = Amount::from(self.literal_bytes);
        let r = Amount::from(self.ranges);
        let k = Amount::from(self.classes);
        let p = Amount::from(self.prefix_calls);
        // The reviewed 64-bit widths also conservatively cover the qualified
        // 32-bit representation. Target admission remains a separate gate.
        let terms = [
            Amount::from(self.smart_allocation),
            n * 80 + u * 128,
            (b + n * 8) * 4 + b + k * 4,
            (r + k * 4) * 32,
            arithmetic::growth(48, n * 2 + 1, 4),
            arithmetic::growth(40, n, 4) + (r + k * 4) * 192,
            (n * 7 + p * 4) * 192 + (n * 2 + p) * 80,
        ];
        let mut sites = [0; 7];
        for (site, term) in sites.iter_mut().zip(terms) {
            *site = term.0?;
        }
        Some(sites)
    }
}

fn leaf(tree: &Ast, totals: &mut Summary) -> Result<Subtree> {
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
            add(&mut totals.classes, 1)?;
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

fn unary(tree: &Ast, child: Subtree, totals: &mut Summary) -> Result<Subtree> {
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
                add(&mut totals.unary, 1)?;
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
            add(&mut totals.unary, 1)?;
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

fn failure(control: &dyn Control) -> Failure {
    if let Err(failure) = control
        .check()
        .and_then(|()| control.charge_work(usize::MAX))
    {
        return failure;
    }
    Failure::new(
        FailureReason::LimitExceeded,
        "preparation-work-units",
        "pattern inventory exceeds its allowance",
    )
}

fn allocation_failure(control: &dyn Control) -> Failure {
    if let Err(failure) = control.check() {
        return failure;
    }
    control.allocation_limit_reached();
    Failure::new(
        FailureReason::LimitExceeded,
        "preparation-allocation-bytes",
        "pattern inventory allocation schedule exceeds its allowance",
    )
}
fn map_error(error: InternalError, control: &dyn Control) -> Error {
    match error {
        InternalError::Representation => Error::Representation,
        InternalError::Overflow => Error::Attempt(failure(control)),
    }
}

/// The scalar certificate is only returned alongside the immutable AST it counts.
pub(super) fn inspect(
    parsed: Parsed,
    control: &dyn Control,
) -> std::result::Result<Inventoried, Error> {
    let summary = walk(&parsed.tree, control)?;
    // Fixed checked schedules, separate from the metadata traversal charge.
    control.charge_work(1024).map_err(Error::Attempt)?;
    let word = size_of::<usize>();
    if !matches!(word, 4 | 8) {
        return Err(Error::TargetLayout);
    }
    let work = summary
        .work_sites(word)
        .and_then(|sites| sites.into_iter().try_fold(0usize, usize::checked_add))
        .ok_or_else(|| Error::Attempt(failure(control)))?;
    let allocation = summary
        .allocation_sites()
        .and_then(|sites| sites.into_iter().try_fold(0usize, usize::checked_add))
        .ok_or_else(|| Error::Attempt(allocation_failure(control)))?;
    control.check().map_err(Error::Attempt)?;
    Ok(Inventoried {
        parsed,
        summary,
        work,
        allocation,
    })
}

fn walk(root: &Ast, control: &dyn Control) -> std::result::Result<Summary, Error> {
    // Initialize fixed walk ownership and publish only the completed root facts.
    let word = size_of::<usize>();
    let entry_work = checked(
        Amount::from(size_of::<Summary>().div_ceil(word)) * 3
            + Amount::from(size_of::<Subtree>().div_ceil(word)) * 2
            + 32,
    )
    .map_err(|error| map_error(error, control))?;
    control.charge_work(entry_work).map_err(Error::Attempt)?;
    control.check().map_err(Error::Attempt)?;
    let mut totals = Summary {
        nodes: 0,
        ..Summary::default()
    };
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut next = Some(root);
    let mut completed = None;
    loop {
        // A node dispatch or one completed-child fold has bounded scalar work.
        // Input width appears only as repeated charged folds, never one batch.
        if next.is_none() && stack.is_empty() {
            let child: Subtree = completed.take().ok_or(Error::Representation)?;
            totals.shape = child.shape;
            totals.alternatives = child.alternatives;
            totals.literal_bytes = child.literal_bytes;
            totals.ranges = child.ranges;
            return Ok(totals);
        }
        let work = if next.is_some() { 96 } else { 128 };
        control.charge_work(work).map_err(Error::Attempt)?;
        control.check().map_err(Error::Attempt)?;
        if let Some(tree) = next.take() {
            add(&mut totals.nodes, 1).map_err(|e| map_error(e, control))?;
            if let Some(frame) = Frame::new(tree) {
                if stack.len() == stack.capacity() {
                    let capacity = stack
                        .capacity()
                        .checked_mul(2)
                        .map(|n| n.max(4))
                        .ok_or_else(|| Error::Attempt(failure(control)))?;
                    let bytes = capacity
                        .checked_mul(size_of::<Frame<'_>>())
                        .filter(|bytes| *bytes <= isize::MAX as usize)
                        .ok_or_else(|| Error::Attempt(allocation_failure(control)))?;
                    let copies = stack
                        .len()
                        .checked_mul(size_of::<Frame<'_>>().div_ceil(size_of::<usize>()))
                        .ok_or_else(|| Error::Attempt(failure(control)))?;
                    control.charge_work(copies).map_err(Error::Attempt)?;
                    control.charge_allocation(bytes).map_err(Error::Attempt)?;
                    stack.reserve_exact(capacity - stack.len());
                }
                // Push/pop and eventual scalar cleanup are prepaid together.
                control
                    .charge_work(
                        size_of::<Frame<'_>>()
                            .div_ceil(word)
                            .checked_mul(3)
                            .ok_or_else(|| Error::Attempt(failure(control)))?,
                    )
                    .map_err(Error::Attempt)?;
                stack.push(frame);
                next = stack.last_mut().and_then(Frame::next);
                if next.is_none() {
                    control
                        .charge_work(stack.last().unwrap().finish_work())
                        .map_err(Error::Attempt)?;
                    completed = Some(
                        stack
                            .pop()
                            .unwrap()
                            .finish(&mut totals)
                            .map_err(|e| map_error(e, control))?,
                    );
                }
            } else {
                completed = Some(leaf(tree, &mut totals).map_err(|e| map_error(e, control))?);
            }
        } else {
            let child = completed.take().ok_or(Error::Representation)?;
            let Some(frame) = stack.last_mut() else {
                return Err(Error::Representation);
            };
            frame
                .accept(child, &mut totals)
                .map_err(|e| map_error(e, control))?;
            next = frame.next();
            if next.is_none() {
                control
                    .charge_work(stack.last().unwrap().finish_work())
                    .map_err(Error::Attempt)?;
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
