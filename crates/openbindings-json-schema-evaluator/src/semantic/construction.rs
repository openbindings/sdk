//! Temporary facts shared while constructing plans in one immutable environment.
//! A positive bit certifies local standard syntax, never a subtree or admission.
use super::*;
use openbindings_internal_schema::Control;
use std::cell::Cell;

pub(super) struct Construction<'a> {
    environment: SchemaEnvironment,
    control: &'a dyn Control,
    // Single-threaded construction shares facts through immutable borrows of
    // its environment. Neither these cells nor the control enter a Plan.
    syntax: Vec<Cell<usize>>,
    limits: Limits,
    programs: patterns::Programs,
    pattern_budget: patterns::compiler::profile::Budget,
}

impl<'a> Construction<'a> {
    pub(super) fn compiled_reading(
        &self,
        source: &str,
    ) -> Result<Option<crate::pattern::Shared>, PreparationFailure> {
        self.programs
            .compiled_reading(source, &self.limits, self.control)
    }
    pub(super) fn new(
        environment: SchemaEnvironment,
        limits: Limits,
        control: &'a dyn Control,
    ) -> Result<Self, PreparationFailure> {
        let nodes = environment.space().node_count();
        let width = usize::BITS as usize;
        let words = (nodes / width)
            .checked_add(usize::from(!nodes.is_multiple_of(width)))
            .ok_or_else(|| allocation_limit(control))?;
        let syntax = allocate_words(words, control)?;
        Ok(Self {
            environment,
            control,
            syntax,
            limits,
            programs: patterns::Programs::default(),
            pattern_budget: patterns::compiler::profile::Budget::default(),
        })
    }

    pub(super) fn environment(&self) -> &SchemaEnvironment {
        &self.environment
    }

    pub(super) fn control(&self) -> &'a dyn Control {
        self.control
    }

    #[cfg(test)]
    pub(super) fn pattern_budget(&self) -> &patterns::compiler::profile::Budget {
        &self.pattern_budget
    }

    pub(super) fn limits(&self) -> &Limits {
        &self.limits
    }

    pub(super) fn pattern(
        &self,
        source: &str,
    ) -> Result<Arc<patterns::Program>, PreparationFailure> {
        self.finish()?;
        let result = self
            .programs
            .add(source, &self.limits, &self.pattern_budget, self.control);
        self.finish()?;
        result
    }

    /// No ignored nested construction failure may publish a prepared Plan.
    pub(super) fn finish(&self) -> Result<(), PreparationFailure> {
        self.pattern_budget.check(self.control)?;
        self.programs.check(self.control)?;
        Ok(())
    }

    /// Call only after the traversal's resource/profile applicability checks.
    /// This operation obtains the actual profile and value from its own space.
    pub(super) fn check_standard(&self, node: usize) -> Result<bool, PreparationFailure> {
        self.control.charge_work(1)?;
        let space = self.environment.space();
        let original = space.node(node);
        let Some(profile) = original.profile.filter(|profile| !profile.is_custom()) else {
            return Err(PreparationFailure::new(
                PreparationFailureKind::UnsupportedCapability {
                    capability: "standard-syntax-profile".into(),
                },
            ));
        };
        let number = space.node_number(node);
        let width = usize::BITS as usize;
        let word = &self.syntax[number / width];
        let bit = 1usize << (number % width);
        if word.get() & bit != 0 {
            return Ok(true);
        }
        let valid = openbindings_internal_schema::fixed_schema::dialect_validity_bounded(
            &original.value,
            profile.edition(),
            self.control,
        )?;
        // A cancelled or exhausted checker must never publish a reusable fact.
        self.control.check()?;
        if valid {
            word.set(word.get() | bit);
        }
        Ok(valid)
    }
}

fn allocation_limit(control: &dyn Control) -> PreparationFailure {
    control.allocation_limit_reached();
    PreparationFailure::new(PreparationFailureKind::Limit {
        resource: "preparation-sdk-reservation-bytes".into(),
    })
}

fn allocate_words(
    words: usize,
    control: &dyn Control,
) -> Result<Vec<Cell<usize>>, PreparationFailure> {
    let bytes = words
        .checked_mul(size_of::<Cell<usize>>())
        .ok_or_else(|| allocation_limit(control))?;
    control.charge_work(words)?;
    control.charge_allocation(bytes)?;
    let mut syntax = Vec::new();
    syntax
        .try_reserve_exact(words)
        .map_err(|_| allocation_limit(control))?;
    syntax.resize_with(words, || Cell::new(0));
    control.check()?;
    Ok(syntax)
}

#[cfg(test)]
mod tests;
