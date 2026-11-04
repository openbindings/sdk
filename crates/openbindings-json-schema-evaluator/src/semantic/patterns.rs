//! Original pattern readings belong to the immutable Plan. Simple matchers are
//! shared directly; VM compilation and mutable search state remain local to a
//! value call until their aggregate prepared-program bounds are qualified.
mod cache;
// Structural construction is qualified in stages before default activation.
#[cfg(test)]
pub(super) mod compiler;
use super::*;
use crate::pattern::{Reading, Unsupported};
use regex_automata::nfa::thompson::{
    self,
    pikevm::{Cache, PikeVM},
};

pub(super) enum Pattern {
    Undefined,
    Unsupported(Unsupported),
    Simple {
        reading: Arc<Reading>,
        work: usize,
    },
    Vm {
        engine: PikeVM,
        cache: Cache,
        work: usize,
    },
}
pub(super) enum ResultKind {
    Matched(bool),
    Undefined,
    Unsupported(Unsupported),
    Unpaired,
}

impl Pattern {
    pub(super) fn prepare(
        reading: &Arc<Reading>,
        limits: &Limits,
        graph: &mut Graph<'_>,
    ) -> Result<Self, NoVerdict> {
        graph.tick()?;
        match reading.as_ref() {
            Reading::Unsupported(Unsupported::Invalid) => Ok(Self::Undefined),
            Reading::Unsupported(Unsupported::Size | Unsupported::Memory(_)) => {
                Err(decision::limit("pattern-size-limit"))
            }
            Reading::Unsupported(why) => Ok(Self::Unsupported(*why)),
            Reading::Linear(linear) => {
                if linear.simple.is_some() {
                    return Ok(Self::Simple {
                        reading: reading.clone(),
                        work: linear.form.len().saturating_add(1),
                    });
                }
                let maximum = limits
                    .total_pattern_memory
                    .min(crate::pattern::COMPILED_SIZE);
                let mut allowance = maximum.min(crate::pattern::FIRST);
                // Compiler scratch includes the AST/HIR and bounded NFA builder
                // before final compact state storage. No DFA or capture tables
                // are constructed by the semantic matcher.
                let engine = loop {
                    // Failed builds consume the same monotone attempt budget;
                    // increase only a demonstrated NFA size refusal.
                    graph.charge(
                        linear
                            .form
                            .len()
                            .saturating_mul(256)
                            .saturating_add(allowance.saturating_mul(32))
                            .saturating_add(4096),
                    )?;
                    graph.work(
                        linear
                            .form
                            .len()
                            .saturating_mul(64)
                            .saturating_add(allowance),
                    )?;
                    match PikeVM::builder()
                        .thompson(
                            thompson::Config::new()
                                .nfa_size_limit(Some(allowance))
                                .which_captures(thompson::WhichCaptures::None),
                        )
                        .build(&linear.form)
                    {
                        Ok(engine) => break engine,
                        Err(error) if error.size_limit().is_some() && allowance < maximum => {
                            allowance = allowance.saturating_mul(2).max(1).min(maximum);
                        }
                        Err(error) if error.size_limit().is_some() => {
                            return Err(decision::limit("pattern-size-limit"));
                        }
                        Err(_) => {
                            return Err(NoVerdict::new(
                                NoVerdictReason::UnsupportedCapability,
                                "pattern-vm-compilation",
                                "the normalized pattern could not be represented by the bounded matcher",
                            ));
                        }
                    }
                };
                // A sparse byte state scans its ranges; a union visits every
                // outgoing epsilon edge. Count those as well as state visits.
                graph.work(engine.get_nfa().states().len())?;
                let work = engine
                    .get_nfa()
                    .states()
                    .iter()
                    .fold(1usize, |total, state| {
                        use thompson::State;
                        let edges = match state {
                            State::Sparse(transitions) => transitions.transitions.len(),
                            State::Union { alternates } => alternates.len(),
                            State::BinaryUnion { .. } => 2,
                            State::Look { .. } => 32,
                            _ => 1,
                        };
                        total.saturating_add(edges.saturating_add(1).saturating_mul(4))
                    });
                let cache = cache::create(&engine, graph)?;
                Ok(Self::Vm {
                    engine,
                    cache,
                    work,
                })
            }
        }
    }
    pub(super) fn evaluate(
        &mut self,
        text: Option<&str>,
        graph: &mut Graph<'_>,
    ) -> Result<ResultKind, NoVerdict> {
        let matched = match self {
            Self::Undefined => return Ok(ResultKind::Undefined),
            Self::Unsupported(why) => return Ok(ResultKind::Unsupported(*why)),
            Self::Simple { reading, work } => {
                let Some(text) = text else {
                    return Ok(ResultKind::Unpaired);
                };
                graph.work(text.len().saturating_add(1).saturating_mul(*work))?;
                let Reading::Linear(linear) = reading.as_ref() else {
                    unreachable!("prepared simple pattern")
                };
                linear
                    .simple
                    .as_ref()
                    .expect("prepared simple matcher")
                    .is_match(text)
            }
            Self::Vm {
                engine,
                cache,
                work,
            } => {
                let Some(text) = text else {
                    return Ok(ResultKind::Unpaired);
                };
                graph.work(text.len().saturating_add(1).saturating_mul(*work))?;
                engine.is_match(cache, text)
            }
        };
        graph.tick()?;
        Ok(ResultKind::Matched(matched))
    }
}
