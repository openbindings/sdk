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
    hash::{DefaultHasher, Hasher},
    sync::Arc,
};

const MAXIMUM_FOOTPRINT: usize = 64 * 1024 * 1024;

struct Form {
    text: String,
    program: Arc<Program>,
}
// Authored source plus the Reader policy. Normalized programs can be shared by
// distinct source spellings, but a source hit must retain its admission policy.
type SourceKey = (usize, u64, usize, usize);
type NormalizedKey = (usize, u64);
// The optional key identifies a new publication that the caller must roll back
// if committing its authored-source index subsequently fails.
type Completed = (Arc<Program>, Option<NormalizedKey>);
#[derive(Default)]
pub(in super::super) struct Programs {
    // Fixed-size ordered keys keep unrelated long forms out of each other's
    // comparison budgets. Hash collisions always compare exact normalized text.
    forms: RefCell<BTreeMap<NormalizedKey, Vec<Form>>>,
    sources: RefCell<BTreeMap<SourceKey, Vec<Form>>>,
    #[cfg(test)]
    forced_hash: Cell<Option<u64>>,
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
    /// Temporary handoff of the completed current interpretation. It is not a
    /// second Reader/engine admission and does not outlive Construction.
    pub(in super::super) fn compiled_reading(
        &self,
        source: &str,
        limits: &Limits,
        control: &dyn Control,
    ) -> Result<Option<pattern::Linear>, PreparationFailure> {
        self.check(control)?;
        control.charge_work(source.len().saturating_add(1))?;
        if pattern::anchored_literal(source).is_some() {
            let Some((work, allocation)) = pattern::preparation_cost(source) else {
                return Ok(None);
            };
            control.charge_work(work)?;
            control.charge_allocation(allocation)?;
            return Ok(
                match pattern::read(source, limits.pattern_depth, limits.pattern_bytes) {
                    Ok(Reading::Linear(linear)) => Some(linear),
                    _ => None,
                },
            );
        }
        let mut hasher = DefaultHasher::new();
        hasher.write(source.as_bytes());
        let key = (
            source.len(),
            hasher.finish(),
            limits.pattern_bytes,
            limits.pattern_depth,
        );
        let height = self.sources.borrow().len().saturating_add(1).ilog2() as usize + 2;
        control.charge_work(height.saturating_mul(12))?;
        let candidates = self.sources.borrow().get(&key).map_or(0, Vec::len);
        let mut program = None;
        for index in 0..candidates {
            control.charge_work(source.len().saturating_add(1))?;
            let sources = self.sources.borrow();
            if sources[&key][index].text == source {
                program = Some(sources[&key][index].program.clone());
                break;
            }
        }
        let Some(program) = program else {
            return Ok(None);
        };
        let count = self.forms.borrow().values().map(Vec::len).sum::<usize>();
        control.charge_work(count.saturating_mul(2).saturating_add(1))?;
        let extent = self
            .forms
            .borrow()
            .values()
            .flatten()
            .find(|form| Arc::ptr_eq(&form.program, &program))
            .map(|form| form.text.len());
        let Some(extent) = extent else {
            return Ok(None);
        };
        let simple = match program.as_ref() {
            Program::Simple { matcher, .. } => Some(matcher),
            Program::Vm { .. } => None,
            _ => return Ok(None),
        };
        control.charge_work(extent.saturating_add(1))?;
        control.charge_allocation(
            extent
                .saturating_add(simple.map_or(0, |m| m.heap_bytes()))
                .saturating_add(256),
        )?;
        let form = self
            .forms
            .borrow()
            .values()
            .flatten()
            .find(|form| Arc::ptr_eq(&form.program, &program))
            .expect("completed form remains owned")
            .text
            .clone();
        let simple = simple.cloned();
        self.check(control)?;
        Ok(Some(pattern::Linear { form, simple }))
    }
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
        // Do not copy an oversized source merely to remember its local refusal.
        if source.len() > limits.pattern_bytes {
            return self.retain(Program::Limited("pattern-byte-limit"), limits, control);
        }
        control.charge_work(source.len().saturating_add(1))?;
        // The direct literal Reader already avoids expansion/parser scratch.
        // Its normalized index is sufficient; retaining another complete text
        // copy would add transient memory without skipping expensive work.
        if pattern::anchored_literal(source).is_some() {
            return self
                .add_normalized(source, limits, budget, control)
                .map(|(p, _)| p);
        }
        control.charge_work(source.len().saturating_add(1))?;
        let mut hasher = DefaultHasher::new();
        hasher.write(source.as_bytes());
        let hash = hasher.finish();
        #[cfg(test)]
        let hash = self.forced_hash.get().unwrap_or(hash);
        let key = (
            source.len(),
            hash,
            limits.pattern_bytes,
            limits.pattern_depth,
        );
        let height = self.sources.borrow().len().saturating_add(1).ilog2() as usize + 2;
        control.charge_work(height.saturating_mul(12))?;
        let candidates = self.sources.borrow().get(&key).map_or(0, Vec::len);
        for index in 0..candidates {
            control.charge_work(height.saturating_mul(12))?;
            control.charge_work(source.len().saturating_add(1))?;
            let found = {
                let sources = self.sources.borrow();
                let candidate = &sources[&key][index];
                (candidate.text == source).then(|| candidate.program.clone())
            };
            if let Some(known) = found {
                self.check(control)?;
                budget.check(control)?;
                return Ok(known);
            }
        }
        // Reserve source ownership and index growth before doing any work that
        // might publish a normalized program. No callbacks run under a borrow.
        control.charge_work(height.saturating_mul(24).saturating_add(candidates + 1))?;
        control.charge_allocation(source.len())?;
        control.charge_allocation(height.saturating_mul(1024))?;
        let (length, capacity) = self
            .sources
            .borrow()
            .get(&key)
            .map_or((0, 0), |b| (b.len(), b.capacity()));
        let requested = if length == capacity {
            let bytes = capacity
                .checked_mul(2)
                .and_then(|n| n.max(4).checked_mul(size_of::<Form>()))
                .ok_or_else(|| self.refuse("pattern-index-memory-limit"))?;
            control.charge_allocation(bytes)?;
            Some(bytes / size_of::<Form>())
        } else {
            None
        };
        control.charge_work(source.len())?;
        let text = source.to_owned();
        let (program, published) = self.add_normalized(source, limits, budget, control)?;
        let completed = (|| {
            {
                let mut sources = self.sources.borrow_mut();
                let bucket = sources.entry(key).or_default();
                if let Some(capacity) = requested
                    && bucket.try_reserve_exact(capacity - bucket.len()).is_err()
                {
                    if bucket.is_empty() {
                        sources.remove(&key);
                    }
                    return Err(self.refuse("pattern-index-memory-limit"));
                }
                bucket.push(Form {
                    text,
                    program: program.clone(),
                });
            }
            self.check(control)?;
            budget.check(control)?;
            Ok(())
        })();
        if let Err(failure) = completed {
            {
                let mut sources = self.sources.borrow_mut();
                if let Some(bucket) = sources.get_mut(&key) {
                    // This append's exact slot identifies the publication;
                    // different source spellings may share the same Arc. The
                    // reservation may have failed before the append occurred.
                    if bucket.len() > candidates {
                        let removed = bucket.remove(candidates);
                        debug_assert_eq!(removed.text, source);
                    }
                    if bucket.is_empty() {
                        sources.remove(&key);
                    }
                }
            }
            if let Some(key) = published {
                let mut forms = self.forms.borrow_mut();
                if let Some(bucket) = forms.get_mut(&key) {
                    bucket.retain(|entry| !Arc::ptr_eq(&entry.program, &program));
                    if bucket.is_empty() {
                        forms.remove(&key);
                    }
                }
            }
            return Err(failure);
        }
        Ok(program)
    }
    fn add_normalized(
        &self,
        source: &str,
        limits: &Limits,
        budget: &compiler::profile::Budget,
        control: &dyn Control,
    ) -> Result<Completed, PreparationFailure> {
        self.check(control)?;
        budget.check(control)?;
        if source.len() > limits.pattern_bytes {
            return self
                .retain(Program::Limited("pattern-byte-limit"), limits, control)
                .map(|p| (p, None));
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
                return self
                    .retain(Program::Limited("pattern-depth-limit"), limits, control)
                    .map(|p| (p, None));
            }
            Err(pattern::Refusal::Invalid) => {
                return self
                    .retain(Program::Undefined, limits, control)
                    .map(|p| (p, None));
            }
            Ok(Reading::Unsupported(Unsupported::Size)) => {
                return self
                    .retain(Program::Limited("pattern-size-limit"), limits, control)
                    .map(|p| (p, None));
            }
            Ok(Reading::Unsupported(why)) => {
                return self
                    .retain(Program::Unsupported(why), limits, control)
                    .map(|p| (p, None));
            }
            Ok(Reading::Linear(linear)) => linear,
        };
        // The fixed policy belongs to Construction. Every hit has still passed
        // source extent, authored depth and Reader admission in this attempt.
        control.charge_work(linear.form.len().saturating_add(1))?;
        let mut hasher = DefaultHasher::new();
        hasher.write(linear.form.as_bytes());
        let hash = hasher.finish();
        #[cfg(test)]
        let hash = self.forced_hash.get().unwrap_or(hash);
        let key = (linear.form.len(), hash);
        let height = self.forms.borrow().len().saturating_add(1).ilog2() as usize + 2;
        control.charge_work(height.saturating_mul(12))?;
        let candidates = self.forms.borrow().get(&key).map_or(0, Vec::len);
        for index in 0..candidates {
            control.charge_work(height.saturating_mul(12))?;
            control.charge_work(linear.form.len().saturating_add(1))?;
            let found = {
                let forms = self.forms.borrow();
                let candidate = &forms[&key][index];
                (candidate.text == linear.form).then(|| candidate.program.clone())
            };
            if let Some(known) = found {
                return Ok((known, None));
            }
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
        // Reserve fixed-key tree growth and collision-bucket replacement
        // backing before publication. The completed form moves into the index.
        // Insertion lookup and a possible rollback lookup/scan are charged
        // before publication; rollback cannot call user control while borrowed.
        control.charge_work(
            height
                .saturating_mul(24)
                .saturating_add(candidates.saturating_add(1)),
        )?;
        control.charge_allocation(height.saturating_mul(1024))?;
        let (length, capacity) = self
            .forms
            .borrow()
            .get(&key)
            .map_or((0, 0), |bucket| (bucket.len(), bucket.capacity()));
        let requested = if length == capacity {
            let capacity = capacity
                .checked_mul(2)
                .and_then(|n| n.max(4).checked_mul(size_of::<Form>()))
                .ok_or_else(|| self.refuse("pattern-index-memory-limit"))?;
            control.charge_allocation(capacity)?;
            Some(capacity / size_of::<Form>())
        } else {
            None
        };
        let program = self.retain(program, limits, control)?;
        {
            let mut forms = self.forms.borrow_mut();
            let bucket = forms.entry(key).or_default();
            if let Some(capacity) = requested
                && bucket.try_reserve_exact(capacity - bucket.len()).is_err()
            {
                if bucket.is_empty() {
                    forms.remove(&key);
                }
                return Err(self.refuse("pattern-index-memory-limit"));
            }
            bucket.push(Form {
                text: linear.form,
                program: program.clone(),
            });
        }
        self.form_bytes.set(form_bytes);
        if let Err(failure) = self.check(control) {
            // Remove only this interrupted publication. Other exact forms in
            // a colliding bucket remain available; aggregate charges stay spent.
            let mut forms = self.forms.borrow_mut();
            if let Some(bucket) = forms.get_mut(&key) {
                bucket.retain(|entry| !Arc::ptr_eq(&entry.program, &program));
                if bucket.is_empty() {
                    forms.remove(&key);
                }
            }
            return Err(failure);
        }
        Ok((program, Some(key)))
    }
}

#[cfg(test)]
mod tests;
