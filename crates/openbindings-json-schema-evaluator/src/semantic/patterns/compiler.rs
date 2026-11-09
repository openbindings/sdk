//! Private one-build construction used by original-node pattern preparation.
mod ast;
mod hir;
mod inventory;
mod nfa;
mod normalized;
pub(crate) mod profile;

use openbindings_internal_schema::{Control, Failure};
use regex_automata::nfa::thompson::pikevm::PikeVM;

#[derive(Debug)]
pub(crate) enum Error {
    ConstructionLimit,
    Representation,
    Size,
    Attempt(Failure),
}
impl From<Failure> for Error {
    fn from(failure: Failure) -> Self {
        Self::Attempt(failure)
    }
}
impl From<hir::Error> for Error {
    fn from(error: hir::Error) -> Self {
        match error {
            hir::Error::ConstructionLimit | hir::Error::TranslationNesting => {
                Self::ConstructionLimit
            }
            hir::Error::Representation => Self::Representation,
            hir::Error::Attempt(failure) => Self::Attempt(failure),
        }
    }
}
impl From<nfa::Error> for Error {
    fn from(error: nfa::Error) -> Self {
        match error {
            nfa::Error::ConstructionLimit => Self::ConstructionLimit,
            nfa::Error::Representation => Self::Representation,
            nfa::Error::Size => Self::Size,
            nfa::Error::Attempt(failure) => Self::Attempt(failure),
        }
    }
}

/// One non-Simple normalized Reader form. The caller already admitted source
/// extent and Reader expansion, and lends its enclosing Construction's budget.
/// The result retains no source, HIR, construction state or mutable search cache.
pub(crate) fn prepare(
    form: &str,
    maximum_bytes: usize,
    budget: &profile::Budget,
    control: &dyn Control,
) -> Result<PikeVM, Error> {
    budget.check(control)?;
    if maximum_bytes == 0 {
        return Err(Error::Size);
    }
    let result = (|| {
        let hir = hir::prepare(form, budget, control)?;
        let result = nfa::prepare(&hir, maximum_bytes, budget, control).map_err(Error::from);
        drop(hir);
        result
    })();
    // Also observe shared failure after a locally refusing stage. No temporary
    // compiler owner survives this boundary, and no failure resets the budget.
    budget.check(control)?;
    result
}

#[cfg(test)]
mod tests;
