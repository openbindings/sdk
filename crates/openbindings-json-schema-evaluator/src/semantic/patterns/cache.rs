//! Requested backing for the locked PikeVM's call-local search cache.
//!
//! One cache stays with its engine: never clone it or reset it to another NFA.
//! The reservation includes cumulative stack growth across every search.
use super::{Cache, Graph, NoVerdict, PikeVM, decision, thompson};
use regex_automata::util::primitives::StateID;

pub(super) fn create(engine: &PikeVM, graph: &mut Graph<'_>) -> Result<Cache, NoVerdict> {
    graph.work(engine.get_nfa().states().len())?;
    let bytes = reservation(engine.get_nfa())
        .ok_or_else(|| decision::limit("semantic-analysis-memory-limit"))?;
    graph.tick()?;
    graph.charge(bytes)?;
    Ok(engine.create_cache())
}

fn reservation(nfa: &thompson::NFA) -> Option<usize> {
    // A state's outgoing edges expand once per closure, after the sparse-set
    // visited check. The first Union edge runs inline; remaining edges stack.
    // Capture restoration is included even though our matcher disables captures.
    let pending = nfa.states().iter().try_fold(1usize, |total, state| {
        let added = match state {
            thompson::State::Union { alternates } => alternates.len().saturating_sub(1),
            thompson::State::BinaryUnion { .. } | thompson::State::Capture { .. } => 1,
            _ => 0,
        };
        total.checked_add(added)
    })?;
    backing(
        nfa.states().len(),
        nfa.group_info().slot_len(),
        nfa.pattern_len(),
        pending,
    )
}

fn backing(states: usize, slots: usize, patterns: usize, pending: usize) -> Option<usize> {
    fn vector_bytes(elements: usize, stride: usize) -> Option<usize> {
        let bytes = elements.checked_mul(stride)?;
        (bytes <= isize::MAX as usize).then_some(bytes)
    }
    // Four sparse arrays and two slot tables, each resized once from empty.
    // Rust's minimum nonzero Vec capacity for these element sizes is four.
    let sparse = vector_bytes(states.max(4), size_of::<StateID>())?.checked_mul(4)?;
    let scratch = slots.max(patterns.checked_mul(2)?);
    let table_len = states.checked_mul(slots)?.checked_add(scratch)?;
    let tables = vector_bytes(table_len.max(4), size_of::<usize>())?.checked_mul(2)?;

    // FollowEpsilon has a tag and at most a SmallIndex plus optional word-sized
    // offset: three words bound its stride on our 32/64-bit targets. Every Vec
    // growth at least doubles capacity; final capacity <=2*max(pending,4), and
    // cumulative requested capacities <2*final. Include reallocations, not just
    // retained capacity. Re-audit on dependency or Rust growth-policy changes.
    let capacity = pending.max(4).checked_mul(2)?;
    let stride = 3 * size_of::<usize>();
    let stack = vector_bytes(capacity, stride)?.checked_mul(2)?;
    sparse.checked_add(tables)?.checked_add(stack)
}

#[cfg(test)]
mod tests;
