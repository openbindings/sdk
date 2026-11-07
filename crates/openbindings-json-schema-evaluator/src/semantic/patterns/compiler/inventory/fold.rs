use super::arithmetic::{Amount, add, checked};
use super::{Ast, InternalError, Result, Root, Shape, Subtree, Summary, unary};

pub(super) struct Frame<'a> {
    tree: &'a Ast,
    index: usize,
    summary: Subtree,
    fold: Fold,
}
enum Fold {
    Unary,
    Concat(Concat),
    Alternate(Alternate),
}
struct Concat {
    nonempty: usize,
    all_literals: bool,
    literal_bytes: usize,
    surviving: Shape,
    prior_literal: bool,
}
struct Alternate {
    frontier: usize,
    ranges: usize,
    prefix: bool,
    first: Shape,
    maybe_singleton: bool,
    definite_other: bool,
    equality_candidate: bool,
}

impl<'a> Frame<'a> {
    pub(super) fn new(tree: &'a Ast) -> Option<Self> {
        let fold = match tree {
            Ast::Group(_) | Ast::Repetition(_) => Fold::Unary,
            Ast::Concat(_) => Fold::Concat(Concat {
                nonempty: 0,
                all_literals: true,
                literal_bytes: 0,
                surviving: Shape::default(),
                prior_literal: false,
            }),
            Ast::Alternation(_) => Fold::Alternate(Alternate {
                frontier: 0,
                ranges: 0,
                prefix: true,
                first: Shape::default(),
                maybe_singleton: true,
                definite_other: false,
                equality_candidate: false,
            }),
            _ => return None,
        };
        let mut summary = Subtree::default();
        if !matches!(fold, Fold::Unary) {
            summary.shape.height = 0;
        }
        if matches!(fold, Fold::Concat(_)) {
            summary.shape.first_branch_weight = 0;
            summary.shape.frontier = 0;
        }
        Some(Self {
            tree,
            index: 0,
            summary,
            fold,
        })
    }
    pub(super) fn next(&mut self) -> Option<&'a Ast> {
        let child = match self.tree {
            Ast::Group(g) if self.index == 0 => Some(&*g.ast),
            Ast::Repetition(r) if self.index == 0 => Some(&*r.ast),
            Ast::Concat(c) => c.asts.get(self.index),
            Ast::Alternation(a) => a.asts.get(self.index),
            _ => None,
        };
        // The index is strictly bounded by an existing slice's length (or 1).
        if child.is_some() {
            self.index += 1;
        }
        child
    }
    pub(super) fn accept(&mut self, child: Subtree) -> Result<()> {
        if matches!(self.fold, Fold::Unary) {
            self.summary = unary(self.tree, child)?;
            return Ok(());
        }
        let s = &mut self.summary;
        let c = child.shape;
        add(&mut s.alternatives, child.alternatives)?;
        add(&mut s.ranges, child.ranges)?;
        add(&mut s.literal_bytes, child.literal_bytes)?;
        s.shape.zero_width_only &= c.zero_width_only;
        s.shape.possible_nonempty |= c.possible_nonempty;
        s.shape.height = s.shape.height.max(c.height);
        match &mut self.fold {
            Fold::Unary => unreachable!(),
            Fold::Concat(f) => {
                let literal = match self.tree {
                    Ast::Concat(concat) => matches!(concat.asts[self.index - 1], Ast::Literal(_)),
                    _ => return Err(InternalError::Representation),
                };
                // Adjacent AST literals form one translator input. A group or
                // singleton-class boundary does not join that lexical run.
                let collapsed = literal && f.prior_literal;
                f.prior_literal = literal;
                if c.root == Root::Literal {
                    add(&mut f.literal_bytes, c.literal_bytes)?;
                }
                if !collapsed {
                    add(&mut s.shape.first_branch_weight, c.first_branch_weight)?;
                    add(&mut s.shape.retained_nodes, c.retained_nodes)?;
                    add(&mut s.shape.frontier, 1)?;
                    if matches!(c.root, Root::Concat | Root::Unknown) {
                        add(&mut s.shape.frontier, c.frontier)?;
                    }
                    if c.root != Root::Empty {
                        add(&mut f.nonempty, 1)?;
                        f.all_literals &= c.root == Root::Literal;
                        f.surviving = c;
                    }
                }
            }
            Fold::Alternate(f) => {
                if self.index == 1 {
                    f.first = c;
                    s.shape.first_branch_weight = checked(Amount::from(c.first_branch_weight) + 1)?;
                } else {
                    f.equality_candidate |=
                        matches!(c.root, Root::Concat | Root::Unknown | Root::Alternate);
                }
                add(&mut s.shape.retained_nodes, c.retained_nodes)?;
                add(
                    &mut f.frontier,
                    match c.root {
                        Root::Alternate => c.frontier,
                        Root::Unknown => c.retained_nodes,
                        _ => 1,
                    },
                )?;
                let ranges = match c.root {
                    Root::Class | Root::Alternate => c.exposed_ranges,
                    Root::Unknown => checked(Amount::from(child.ranges) + c.retained_nodes)?,
                    _ => 0,
                };
                add(&mut f.ranges, ranges)?;
                f.prefix &= match c.root {
                    Root::Alternate => c.all_concat,
                    Root::Concat | Root::Unknown => true,
                    _ => false,
                };
                f.maybe_singleton &= !(matches!(
                    c.root,
                    Root::Empty | Root::Look | Root::Unary | Root::Class | Root::Alternate
                ) || (c.root == Root::Literal && c.literal_bytes > 4));
                f.definite_other |= matches!(
                    c.root,
                    Root::Unary | Root::Look | Root::Empty | Root::Alternate
                ) || (c.root == Root::Literal && c.literal_bytes > 4);
            }
        }
        Ok(())
    }
    pub(super) fn finish(mut self, totals: &mut Summary) -> Result<Subtree> {
        match self.fold {
            Fold::Unary => {}
            Fold::Concat(f) => {
                let s = &mut self.summary;
                let visited = s.shape.frontier;
                s.shape.first_branch_weight = s.shape.first_branch_weight.max(1);
                s.shape.height = checked(Amount::from(s.shape.height) + 1)?;
                add(&mut totals.pressure.reconstruction_entries, visited)?;
                add(&mut totals.pressure.literal_byte_revisits, s.literal_bytes)?;
                if f.nonempty == 0 {
                    s.shape.root = Root::Empty;
                    s.shape.height = 1;
                    s.shape.retained_nodes = 1;
                    s.shape.frontier = 1;
                } else if f.all_literals {
                    s.shape.root = Root::Literal;
                    s.shape.height = 1;
                    s.shape.literal_bytes = f.literal_bytes;
                    s.shape.retained_nodes = 1;
                    s.shape.frontier = 1;
                } else if f.nonempty == 1 {
                    let c = f.surviving;
                    s.shape.root = c.root;
                    s.shape.retained_nodes = c.retained_nodes;
                    s.shape.frontier = c.frontier;
                    s.shape.exposed_ranges = c.exposed_ranges;
                    s.shape.all_concat = c.all_concat;
                    s.shape.literal_bytes = c.literal_bytes;
                } else {
                    s.shape.root = Root::Concat;
                    s.shape.exposed_ranges =
                        checked(Amount::from(s.ranges) + s.shape.retained_nodes)?;
                }
            }
            Fold::Alternate(f) => {
                let s = &mut self.summary;
                add(&mut s.alternatives, 1)?;
                let calls = if f.prefix {
                    checked(Amount::from(s.alternatives.min(f.first.first_branch_weight)) + 1)?
                } else {
                    1
                };
                let entering_height = s.shape.height;
                s.shape.height = checked(
                    Amount::from(entering_height)
                        + if f.prefix {
                            checked(Amount::from(calls) * 2 + 1)?
                        } else {
                            1
                        },
                )?;
                totals.maximum_prefix_calls = totals.maximum_prefix_calls.max(calls);
                if matches!(f.first.root, Root::Concat | Root::Unknown) && f.equality_candidate {
                    totals.equality_height = totals.equality_height.max(entering_height);
                }
                let nodes = Amount::from(s.shape.retained_nodes);
                let bytes = Amount::from(s.literal_bytes);
                let frontier = if f.prefix {
                    checked(nodes * 2)?
                } else {
                    f.frontier
                };
                let ranges = if f.prefix {
                    checked(
                        Amount::from(s.ranges)
                            + if s.alternatives > 1 {
                                s.literal_bytes
                            } else {
                                0
                            },
                    )?
                } else {
                    f.ranges
                };
                let front = Amount::from(frontier);
                let range = Amount::from(ranges);
                // These are conservative structural pressure scores. Equality
                // may be attempted before a later non-concat prevents prefix
                // reconstruction, so its node/byte pressure is unconditional.
                add(
                    &mut totals.pressure.reconstruction_entries,
                    checked(front * calls + nodes * 2 * calls)?,
                )?;
                add(
                    &mut totals.pressure.literal_byte_revisits,
                    checked(bytes * calls)?,
                )?;
                if f.prefix {
                    add(
                        &mut totals.pressure.reconstruction_entries,
                        checked(nodes * 6 * calls)?,
                    )?;
                    add(
                        &mut totals.pressure.literal_byte_revisits,
                        checked(bytes * 2 * calls)?,
                    )?;
                }
                if f.prefix || f.maybe_singleton {
                    add(
                        &mut totals.pressure.candidate_range_slots,
                        checked(front * 32)?,
                    )?;
                }
                if ranges > 0 {
                    add(
                        &mut totals.pressure.candidate_range_slots,
                        checked(front * 64 * range)?,
                    )?;
                }
                if !f.prefix && f.definite_other {
                    s.shape.root = Root::Alternate;
                    s.shape.frontier = f.frontier;
                    s.shape.exposed_ranges = f.ranges;
                    s.shape.all_concat = false;
                } else {
                    s.shape.root = Root::Unknown;
                    s.shape.frontier = s.shape.retained_nodes;
                    s.shape.exposed_ranges =
                        checked(Amount::from(s.ranges) + s.shape.retained_nodes)?;
                    s.shape.all_concat = true;
                }
            }
        }
        Ok(self.summary)
    }
}
