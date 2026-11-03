//! One parsed pattern and bounded matcher per original operand per analysis.
//! VM state is local to this run; repeated instance occurrences reuse it without
//! another parse/compile or an unbounded per-thread search-cache allocation.
use super::*;
use crate::pattern::{Reading, Refusal, Simple, Unsupported};
use regex_automata::nfa::thompson::{
    self,
    pikevm::{Cache, PikeVM},
};

pub(super) enum Pattern {
    Undefined,
    Unsupported(Unsupported),
    Simple {
        matcher: Simple,
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
        source: &str,
        limits: &Limits,
        graph: &mut Graph<'_>,
    ) -> Result<Self, NoVerdict> {
        graph.work(source.len().saturating_add(1))?;
        let (work, allocation) = crate::pattern::preparation_cost(source)
            .ok_or_else(|| decision::limit("preparation-allocation-bytes"))?;
        graph.work(work)?;
        graph.charge(allocation)?;
        match crate::pattern::read(source, limits.pattern_depth, limits.pattern_bytes) {
            Err(Refusal::Invalid) | Ok(Reading::Unsupported(Unsupported::Invalid)) => {
                Ok(Self::Undefined)
            }
            Err(Refusal::Depth) => Err(decision::limit("pattern-depth-limit")),
            Ok(Reading::Unsupported(Unsupported::Size | Unsupported::Memory(_))) => {
                Err(decision::limit("pattern-size-limit"))
            }
            Ok(Reading::Unsupported(why)) => Ok(Self::Unsupported(why)),
            Ok(Reading::Linear(linear)) => {
                if let Some(matcher) = linear.simple {
                    return Ok(Self::Simple {
                        matcher,
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
                // Two sparse active sets, their slot tables, and the iterative
                // epsilon stack are bounded by the exact completed state count.
                graph.charge(
                    engine
                        .get_nfa()
                        .states()
                        .len()
                        .saturating_mul(256)
                        .saturating_add(4096),
                )?;
                // A sparse byte state scans its ranges; a union visits every
                // outgoing epsilon edge. Count those as well as state visits.
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
                let cache = engine.create_cache();
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
            Self::Simple { matcher, work } => {
                let Some(text) = text else {
                    return Ok(ResultKind::Unpaired);
                };
                graph.work(text.len().saturating_add(1).saturating_mul(*work))?;
                matcher.is_match(text)
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
