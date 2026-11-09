//! One forward, capture-free construction from an already admitted HIR.
//!
//! Its caller must first admit Reader expansion,
//! AST/HIR breadth and reconstruction. This stage admits actual-HIR path, helper
//! payload and join pressure, then debits the shared aggregate before building.
use super::profile;
use openbindings_internal_schema::{Control, Failure};
use regex_automata::nfa::thompson::{self, pikevm::PikeVM};
use regex_syntax::hir::{Hir, HirKind};
use std::mem::size_of;
mod admission;

// Named recursive compiler dispatches, not a portable byte-stack promise.
const MAXIMUM_PATH: usize = super::profile::NFA_PATH;

#[derive(Debug)]
pub(super) enum Error {
    ConstructionLimit,
    Size,
    Representation,
    Attempt(Failure),
}
impl From<profile::AdmissionError> for Error {
    fn from(error: profile::AdmissionError) -> Self {
        match error {
            profile::AdmissionError::PatternLimit => Self::ConstructionLimit,
            profile::AdmissionError::Attempt(failure) => Self::Attempt(failure),
        }
    }
}
impl From<Failure> for Error {
    fn from(failure: Failure) -> Self {
        Self::Attempt(failure)
    }
}

struct Ancestor<'a> {
    remaining: &'a [Hir],
    path: usize,
}

/// Count live named recursive helpers in locked regex-automata's forward
/// compiler. Literal alternatives use its iterative trie and are descent leaves.
/// The traversal retains ancestors only, never a width-sized metadata tree.
fn compiler_path(hir: &Hir, control: &dyn Control) -> Result<usize, Error> {
    let mut ancestors: Vec<Ancestor<'_>> = Vec::new();
    let (mut node, mut prefix, mut maximum) = (hir, 0, 0);
    loop {
        control.charge_work(1)?;
        let (local, child_offset, children) = match node.kind() {
            HirKind::Capture(capture) => (1, 2, std::slice::from_ref(capture.sub.as_ref())),
            HirKind::Concat(children) => (1, 2, children.as_slice()),
            HirKind::Alternation(children) => {
                control.charge_work(children.len())?;
                if children.len() >= 2
                    && children
                        .iter()
                        .all(|child| matches!(child.kind(), HirKind::Literal(_)))
                {
                    (2, 0, &[][..])
                } else {
                    (1, 3, children.as_slice())
                }
            }
            HirKind::Repetition(rep) => {
                let child = std::slice::from_ref(rep.sub.as_ref());
                match (rep.min, rep.max) {
                    (0, Some(0)) => (4, 0, &[][..]),
                    (0, Some(1)) => (1, 3, child),
                    (min, Some(max)) if min == max => (1, 4, child),
                    (0, Some(_)) => (5, 3, child),
                    (_, Some(_)) => (1, 5, child),
                    (0 | 1, None) => (1, 3, child),
                    (_, None) => (1, 5, child),
                }
            }
            _ => (1, 0, &[][..]),
        };
        maximum = maximum.max(prefix + local);
        if maximum > MAXIMUM_PATH || (!children.is_empty() && prefix + child_offset >= MAXIMUM_PATH)
        {
            control.check()?;
            return Err(Error::ConstructionLimit);
        }
        if let Some((first, remaining)) = children.split_first() {
            let path = prefix + child_offset;
            if !remaining.is_empty() {
                // All arithmetic is bounded by the logical path ceiling. Charge
                // SDK-owned ancestor storage before growth; dependency storage
                // is governed separately, not certified by this reservation.
                if ancestors.len() == ancestors.capacity() {
                    let capacity = (ancestors.capacity().max(4) * 2).min(MAXIMUM_PATH);
                    if ancestors.len() >= capacity {
                        control.check()?;
                        return Err(Error::ConstructionLimit);
                    }
                    control.charge_allocation(capacity * size_of::<Ancestor<'_>>())?;
                    if ancestors
                        .try_reserve_exact(capacity - ancestors.len())
                        .is_err()
                    {
                        control.check()?;
                        return Err(Error::Size);
                    }
                }
                ancestors.push(Ancestor { remaining, path });
            }
            (node, prefix) = (first, path);
            continue;
        }
        match ancestors.last_mut() {
            Some(ancestor) => {
                let (next, rest) = ancestor.remaining.split_first().expect("nonempty siblings");
                (node, prefix) = (next, ancestor.path);
                ancestor.remaining = rest;
                if rest.is_empty() {
                    ancestors.pop();
                }
            }
            None => break,
        }
    }
    control.check()?;
    Ok(maximum)
}

fn build_error(error: thompson::BuildError) -> Error {
    // Do not format or retain a dependency diagnostic or its construction data.
    if error.size_limit().is_some() {
        Error::Size
    } else {
        Error::Representation
    }
}

/// The outer factory must apply the supported structural profile before calling
/// this stage. `maximum_bytes` limits backend representation, not transient
/// allocator requests. Aggregate unique-program ownership is checked by the
/// outer owner before publication. No HIR, control or search cache is retained.
pub(super) fn prepare(
    hir: &Hir,
    maximum_bytes: usize,
    budget: &profile::Budget,
    control: &dyn Control,
) -> Result<PikeVM, Error> {
    budget.check(control)?;
    if maximum_bytes == 0 {
        return Err(Error::Size);
    }
    let scores = admission::inspect(hir, control)?;
    budget.nfa(
        scores.dispatch,
        scores.helper_payload,
        scores.empty_tail_pressure,
        control,
    )?;
    let maximum_bytes = maximum_bytes.min(profile::NFA_BYTES);
    budget.check(control)?;
    let built = {
        #[cfg(test)]
        tests::entered_backend();
        let mut compiler = thompson::NFA::compiler();
        compiler.configure(
            thompson::Config::new()
                .utf8(true)
                .reverse(false)
                .which_captures(thompson::WhichCaptures::None)
                .nfa_size_limit(Some(maximum_bytes)),
        );
        compiler.build_from_hir(hir).map_err(build_error)
    };
    // The fresh compiler and any dependency error have been dropped. A sticky
    // attempt failure or late cancellation takes precedence over local refusal.
    budget.check(control)?;
    let nfa = built?;
    if nfa.memory_usage() > maximum_bytes {
        return Err(Error::Size);
    }
    budget.check(control)?;
    let built = {
        #[cfg(test)]
        tests::entered_wrapper();
        PikeVM::builder().build_from_nfa(nfa).map_err(build_error)
    };
    budget.check(control)?;
    built
}

#[cfg(test)]
mod tests;
