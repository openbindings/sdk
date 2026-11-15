//! Immutable pattern interpretations belong to the Plan. Construction shares
//! completed normalized programs; each reached value call owns its search cache.
mod cache;
pub(super) mod compiler;
mod preparation;
use super::*;
use crate::pattern::{Simple, Unsupported};
pub(super) use preparation::Programs;
use regex_automata::nfa::thompson::{
    self,
    pikevm::{Cache, PikeVM},
};

#[derive(Debug)]
pub(super) enum Program<M = Arc<Simple>> {
    Undefined,
    Unsupported(Unsupported),
    Simple {
        matcher: M,
        work: usize,
        bytes: usize,
    },
    Vm {
        engine: PikeVM,
        work: usize,
        bytes: usize,
    },
    Limited(&'static str),
}
impl Program {
    /// Defined retained representation footprint, not allocator-wide bytes.
    /// NFA memory_usage counts representation lengths, not Vec capacities.
    fn footprint(&self) -> Option<usize> {
        let backing = match self {
            Self::Simple { matcher, .. } => matcher.heap_bytes(),
            Self::Vm { engine, .. } => engine
                .get_nfa()
                .memory_usage()
                .checked_add(2 * size_of::<usize>())?,
            _ => 0,
        };
        Self::owner_bytes().checked_add(backing)
    }
    fn allocation_bytes(&self) -> Option<usize> {
        let owner = 16 + size_of::<Self>();
        let backing = match self {
            Self::Simple { matcher, .. } => 16 + size_of::<Simple>() + matcher.heap_bytes(),
            Self::Vm { engine, .. } => 16 + engine.get_nfa().memory_usage(),
            _ => 0,
        };
        owner.checked_add(backing)
    }
    fn owner_bytes() -> usize {
        // Keep the inline matcher footprint for admission, so shared ownership
        // does not change the existing pattern memory or reservation limits.
        std::alloc::Layout::new::<[usize; 2]>()
            .extend(std::alloc::Layout::new::<Program<Simple>>())
            .expect("fixed program owner layout")
            .0
            .pad_to_align()
            .size()
    }
}

pub(super) struct Search {
    program: Arc<Program>,
    cache: Option<Cache>,
}
pub(super) enum ResultKind {
    Matched(bool),
    Undefined,
    Unsupported(Unsupported),
    Unpaired,
}

impl Search {
    pub(super) fn new(program: &Arc<Program>, graph: &mut Graph<'_>) -> Result<Self, NoVerdict> {
        graph.tick()?;
        let cache = match program.as_ref() {
            Program::Vm { engine, .. } => Some(cache::create(engine, graph)?),
            Program::Limited(code) => return Err(decision::limit(code)),
            _ => None,
        };
        graph.tick()?;
        Ok(Self {
            program: program.clone(),
            cache,
        })
    }
    pub(super) fn evaluate(
        &mut self,
        text: Option<&str>,
        maximum_work: Option<usize>,
        graph: &mut Graph<'_>,
    ) -> Result<ResultKind, NoVerdict> {
        let matched = match self.program.as_ref() {
            Program::Undefined => return Ok(ResultKind::Undefined),
            Program::Unsupported(why) => return Ok(ResultKind::Unsupported(*why)),
            Program::Limited(code) => return Err(decision::limit(code)),
            Program::Simple {
                matcher,
                work,
                bytes,
            } => {
                let Some(text) = text else {
                    return Ok(ResultKind::Unpaired);
                };
                charge_search(text.len().min(*bytes), *work, maximum_work, graph)?;
                matcher.is_match(text)
            }
            Program::Vm {
                engine,
                work,
                bytes,
            } => {
                let Some(text) = text else {
                    return Ok(ResultKind::Unpaired);
                };
                let mut end = text.len();
                // An unanchored positive match in a bounded prefix proves a
                // match in the whole input. Retain the complete haystack for
                // look assertions: slicing would invent an end-of-input.
                if !engine.get_nfa().is_always_start_anchored()
                    && let Some(maximum) = maximum_work
                {
                    end = end.min((maximum / work).saturating_sub(1));
                }
                charge_search(end.min(*bytes), *work, maximum_work, graph)?;
                let matched = engine.is_match(
                    self.cache.as_mut().expect("call-local VM cache"),
                    regex_automata::Input::new(text).span(0..end),
                );
                if !matched && end < text.len() {
                    graph.tick()?;
                    return Err(decision::limit("pattern-search-work-limit"));
                }
                matched
            }
        };
        graph.tick()?;
        Ok(ResultKind::Matched(matched))
    }
}

fn charge_search(
    bytes: usize,
    work: usize,
    maximum: Option<usize>,
    graph: &mut Graph<'_>,
) -> Result<(), NoVerdict> {
    let units = (bytes as u64 + 1).saturating_mul(work as u64);
    // Keep cancellation, shared preparation and the caller's cumulative cap
    // precedence. The per-search ceiling prevents entry into either matcher.
    graph.work_units(units)?;
    if maximum.is_some_and(|maximum| units > maximum as u64) {
        return Err(decision::limit("pattern-search-work-limit"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
