//! One temporary normalized-program index in the enclosing Construction.
use super::{Program, compiler};
use crate::{
    Limits,
    pattern::{self, Reading, Unsupported},
};
use openbindings::{PreparationFailure, PreparationFailureKind};
use openbindings_internal_schema::Control;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Arc,
};

const MAXIMUM_FOOTPRINT: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(in super::super) struct Programs {
    forms: RefCell<BTreeMap<String, Arc<Program>>>,
    form_bytes: Cell<usize>,
    footprint: Cell<usize>,
    failure: Cell<Option<&'static str>>,
}
fn limit(resource: &'static str) -> PreparationFailure {
    PreparationFailure::new(PreparationFailureKind::Limit {
        resource: resource.into(),
    })
}
impl Programs {
    pub(in super::super) fn check(&self, control: &dyn Control) -> Result<(), PreparationFailure> {
        if let Some(resource) = self.failure.get() {
            return Err(limit(resource));
        }
        control.check()?;
        Ok(())
    }
    fn refuse(&self, resource: &'static str) -> PreparationFailure {
        let resource = self.failure.get().unwrap_or(resource);
        self.failure.set(Some(resource));
        limit(resource)
    }
    fn retain(
        &self,
        program: Program,
        limits: &Limits,
        control: &dyn Control,
    ) -> Result<Arc<Program>, PreparationFailure> {
        self.check(control)?;
        let total = program
            .footprint()
            .and_then(|n| self.footprint.get().checked_add(n))
            .filter(|n| *n <= limits.total_pattern_memory.min(MAXIMUM_FOOTPRINT))
            .ok_or_else(|| self.refuse("pattern-total-memory-limit"))?;
        control.charge_allocation(Program::owner_bytes())?;
        let owner = Arc::new(program);
        control.check()?;
        self.footprint.set(total);
        Ok(owner)
    }
    pub(in super::super) fn add(
        &self,
        source: &str,
        limits: &Limits,
        budget: &compiler::profile::Budget,
        control: &dyn Control,
    ) -> Result<Arc<Program>, PreparationFailure> {
        self.check(control)?;
        budget.check(control)?;
        if source.len() > limits.pattern_bytes {
            return self.retain(Program::Limited("pattern-byte-limit"), limits, control);
        }
        // Expansion and repeated class union precede rendered-form limits.
        control.charge_work(source.len().saturating_add(1))?;
        let (work, allocation) = pattern::preparation_cost(source)
            .ok_or_else(|| self.refuse("preparation-sdk-reservation-bytes"))?;
        control.charge_work(work)?;
        control.charge_allocation(allocation)?;
        let reading = pattern::read(source, limits.pattern_depth, limits.pattern_bytes);
        control.check()?;
        let linear = match reading {
            Err(pattern::Refusal::Depth) => {
                return self.retain(Program::Limited("pattern-depth-limit"), limits, control);
            }
            Err(pattern::Refusal::Invalid) | Ok(Reading::Unsupported(Unsupported::Invalid)) => {
                return self.retain(Program::Undefined, limits, control);
            }
            Ok(Reading::Unsupported(Unsupported::Size | Unsupported::Memory(_))) => {
                return self.retain(Program::Limited("pattern-size-limit"), limits, control);
            }
            Ok(Reading::Unsupported(why)) => {
                return self.retain(Program::Unsupported(why), limits, control);
            }
            Ok(Reading::Linear(linear)) => linear,
        };
        // The fixed policy belongs to Construction. Every hit has still passed
        // source extent, authored depth and Reader admission in this attempt.
        let height = self.forms.borrow().len().saturating_add(1).ilog2() as usize + 2;
        control.charge_work(
            linear
                .form
                .len()
                .saturating_add(1)
                .saturating_mul(height)
                .saturating_mul(12),
        )?;
        if let Some(known) = self.forms.borrow().get(&linear.form).cloned() {
            return Ok(known);
        }
        let form_bytes = self
            .form_bytes
            .get()
            .checked_add(linear.form.len())
            .filter(|n| *n <= limits.total_pattern_bytes)
            .ok_or_else(|| self.refuse("pattern-total-byte-limit"))?;
        let program = if let Some(matcher) = linear.simple {
            Program::Simple {
                matcher,
                work: linear.form.len().saturating_add(1),
            }
        } else {
            match compiler::prepare(&linear.form, compiler::profile::NFA_BYTES, budget, control) {
                Ok(engine) => {
                    control.charge_work(engine.get_nfa().states().len())?;
                    let work = engine
                        .get_nfa()
                        .states()
                        .iter()
                        .try_fold(1usize, |total, state| {
                            use regex_automata::nfa::thompson::State;
                            let edges = match state {
                                State::Sparse(transitions) => transitions.transitions.len(),
                                State::Union { alternates } => alternates.len(),
                                State::BinaryUnion { .. } => 2,
                                State::Look { .. } => 32,
                                _ => 1,
                            };
                            total.checked_add(edges.checked_add(1)?.checked_mul(4)?)
                        });
                    match work {
                        Some(work) => Program::Vm { engine, work },
                        None => Program::Limited("pattern-size-limit"),
                    }
                }
                Err(compiler::Error::Attempt(failure)) => return Err(failure.into()),
                Err(compiler::Error::ConstructionLimit | compiler::Error::Size) => {
                    Program::Limited("pattern-size-limit")
                }
                Err(compiler::Error::Representation) => {
                    Program::Limited("pattern-representation-limit")
                }
            }
        };
        budget.check(control)?;
        // Bound SDK-owned BTree node splits (at most one per level plus root).
        // One charged temporary key permits rollback if allocation cancels.
        // Only the index String survives; no form enters the Program or Plan.
        control.charge_allocation(height.saturating_mul(1024))?;
        let program = self.retain(program, limits, control)?;
        control.charge_allocation(linear.form.len())?;
        self.forms
            .borrow_mut()
            .insert(linear.form.clone(), program.clone());
        self.form_bytes.set(form_bytes);
        if let Err(failure) = self.check(control) {
            // An allocation/callback can cancel after the pre-publication check.
            // Do not let a later lookup observe that interrupted insertion.
            self.forms.borrow_mut().remove(&linear.form);
            return Err(failure);
        }
        Ok(program)
    }
}

#[cfg(test)]
mod tests;
