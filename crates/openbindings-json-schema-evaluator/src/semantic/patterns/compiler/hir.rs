//! One structurally admitted translation, retaining only immutable HIR.
use super::{ast, inventory, profile};
use openbindings_internal_schema::{Control, Failure};
use regex_syntax::hir::{Hir, translate::TranslatorBuilder};

#[derive(Debug)]
pub(super) enum Error {
    Representation,
    ConstructionLimit,
    TranslationNesting,
    Attempt(Failure),
}
impl From<Failure> for Error {
    fn from(failure: Failure) -> Self {
        Self::Attempt(failure)
    }
}
impl From<ast::Error> for Error {
    fn from(error: ast::Error) -> Self {
        match error {
            ast::Error::Attempt(failure) => Self::Attempt(failure),
            ast::Error::Refused(ast::Refusal::ConstructionLimit) => Self::ConstructionLimit,
            ast::Error::Refused(_) => Self::Representation,
        }
    }
}
impl From<inventory::Error> for Error {
    fn from(error: inventory::Error) -> Self {
        match error {
            inventory::Error::Attempt(failure) => Self::Attempt(failure),
            inventory::Error::ConstructionLimit => Self::ConstructionLimit,
            inventory::Error::Representation => Self::Representation,
        }
    }
}

/// The same normalized borrow supplies guard, parser and translator. Reader's
/// source/expansion admission has already applied. Aggregate construction pressure
/// must additionally be debited by the enclosing once-prepared-program owner.
/// Dependency execution is not represented as a complete SDK work/byte ledger.
pub(super) fn prepare(form: &str, control: &dyn Control) -> Result<Hir, Error> {
    control.check()?;
    let parsed = ast::prepare(
        form,
        profile::NORMALIZED_GROUPS,
        profile::AST_DEPTH,
        control,
    )?;
    let owned = inventory::inspect(parsed, control)?;
    if owned.summary.maximum_prefix_calls > profile::PREFIX_CALLS
        || owned.summary.equality_height > profile::EQUALITY_HEIGHT
    {
        control.check()?;
        return Err(Error::TranslationNesting);
    }
    if !profile::admits_translation(&owned.summary.pressure) {
        control.check()?;
        return Err(Error::ConstructionLimit);
    }
    control.check()?;
    let output = {
        #[cfg(test)]
        tests::entered_translator();
        let mut translator = TranslatorBuilder::new().utf8(true).build();
        translator
            .translate(form, &owned.parsed.tree)
            .map_err(|error| {
                drop(error);
                Error::Representation
            })
    };
    drop(owned);
    control.check()?;
    output
}

#[cfg(test)]
mod tests;
