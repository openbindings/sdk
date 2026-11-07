//! Temporary aggregate structural admission for one enclosing Construction.
//! The semantic Construction owns exactly one of these and checks it before
//! publishing any Plan. Nested/selected-meta construction borrows the same owner.
use super::*;
use openbindings_internal_schema::{Control, Failure, FailureReason};
use std::cell::Cell;

#[derive(Clone, Copy, Default)]
struct Counts {
    ast_nodes: usize,
    class_items: usize,
    reconstruction: usize,
    literal_revisits: usize,
    range_slots: usize,
    dispatch: usize,
    helper: usize,
    tail: usize,
}
const AGGREGATE: Counts = Counts {
    ast_nodes: 262_144,
    class_items: 262_144,
    reconstruction: 4_194_304,
    literal_revisits: 8_388_608,
    range_slots: 67_108_864,
    dispatch: 1_048_576,
    helper: 8_388_608,
    tail: 8_388_608,
};
#[derive(Debug)]
pub(crate) enum Error {
    PatternLimit,
    Attempt(Failure),
}
impl From<Failure> for Error {
    fn from(failure: Failure) -> Self {
        Self::Attempt(failure)
    }
}

#[derive(Default)]
pub(crate) struct Budget {
    used: Cell<Counts>,
    // Static resource identity keeps this first cause allocation-free.
    failure: Cell<Option<&'static str>>,
}
fn exhausted(resource: &'static str) -> Failure {
    Failure::new(
        FailureReason::LimitExceeded,
        resource,
        "pattern construction exceeded its shared structural allowance",
    )
}
impl Budget {
    /// Check at outer completion even if a nested construction error was ignored.
    /// A previously observed structural failure precedes later local refusals.
    /// The enclosing SDK Attempt retains its own final cancellation precedence.
    pub(crate) fn check(&self, control: &dyn Control) -> Result<(), Failure> {
        match self.failure.get() {
            Some(resource) => Err(exhausted(resource)),
            None => control.check(),
        }
    }
    pub(in super::super) fn ast(
        &self,
        summary: &normalized::Summary,
        control: &dyn Control,
    ) -> Result<(), Error> {
        self.check(control)?;
        if !admits_normalized(summary) {
            return Err(Error::PatternLimit);
        }
        self.debit(
            Counts {
                ast_nodes: ast_nodes(summary).expect("admitted exact node bound"),
                class_items: summary.class_items,
                ..Counts::default()
            },
            control,
        )
    }
    pub(in super::super) fn hir(
        &self,
        pressure: &Pressure,
        control: &dyn Control,
    ) -> Result<(), Error> {
        self.check(control)?;
        if !admits_translation(pressure) {
            return Err(Error::PatternLimit);
        }
        self.debit(
            Counts {
                reconstruction: pressure.reconstruction_entries,
                literal_revisits: pressure.literal_byte_revisits,
                range_slots: pressure.candidate_range_slots,
                ..Counts::default()
            },
            control,
        )
    }
    pub(crate) fn nfa(
        &self,
        dispatch: usize,
        helper: usize,
        tail: usize,
        control: &dyn Control,
    ) -> Result<(), Error> {
        self.check(control)?;
        if dispatch > NFA_DISPATCH || helper > NFA_HELPER_PAYLOAD || tail > NFA_EMPTY_TAIL {
            return Err(Error::PatternLimit);
        }
        self.debit(
            Counts {
                dispatch,
                helper,
                tail,
                ..Counts::default()
            },
            control,
        )
    }
    fn debit(&self, amount: Counts, control: &dyn Control) -> Result<(), Error> {
        self.check(control)?;
        let mut total = self.used.get();
        macro_rules! charge {
            ($field:ident, $code:literal) => {
                match total
                    .$field
                    .checked_add(amount.$field)
                    .filter(|n| *n <= AGGREGATE.$field)
                {
                    Some(value) => total.$field = value,
                    None => {
                        self.failure.set(Some($code));
                        return Err(Error::Attempt(exhausted($code)));
                    }
                }
            };
        }
        charge!(ast_nodes, "pattern-construction-ast-nodes");
        charge!(class_items, "pattern-construction-class-items");
        charge!(reconstruction, "pattern-construction-reconstruction");
        charge!(literal_revisits, "pattern-construction-literal-revisits");
        charge!(range_slots, "pattern-construction-range-slots");
        charge!(dispatch, "pattern-construction-dispatch");
        charge!(helper, "pattern-construction-helper-payload");
        charge!(tail, "pattern-construction-empty-tail");
        self.used.set(total);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
