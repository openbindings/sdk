//! One guarded translation owner, admitted before recursive smart constructors.
use super::{ast, inventory};
use openbindings_internal_schema::{Control, Failure, FailureReason};
use regex_syntax::hir::{
    Hir,
    translate::{Translator, TranslatorBuilder},
};
use std::mem::size_of;

// Logical construction limits, not byte estimates for an arbitrary Rust build.
const PREFIX_CALLS: usize = 64;
const EQUALITY_HEIGHT: usize = 256;
const AST_REPRESENTATION_DEPTH: u32 = 250;

// Fixed scalar admission/error/control operations plus at most four logical
// word transfers per owner across local/result slots. Dependency traversal,
// allocation and destruction have their own prepaid schedules.
fn entry_work() -> Option<usize> {
    let owners = [
        size_of::<ast::Parsed>(),
        size_of::<inventory::Inventoried>(),
        size_of::<TranslatorBuilder>(),
        size_of::<Translator>(),
        size_of::<Result<Hir, regex_syntax::hir::Error>>(),
        size_of::<Result<Hir, Error>>(),
    ];
    owners.into_iter().try_fold(128usize, |work, bytes| {
        work.checked_add(bytes.div_ceil(size_of::<usize>()).checked_mul(4)?)
    })
}

#[derive(Debug)]
pub(super) enum Error {
    Representation,
    TargetLayout,
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
            ast::Error::Refused(ast::Refusal::TargetLayout) => Self::TargetLayout,
            ast::Error::Refused(_) => Self::Representation,
        }
    }
}
impl From<inventory::Error> for Error {
    fn from(error: inventory::Error) -> Self {
        match error {
            inventory::Error::Attempt(failure) => Self::Attempt(failure),
            inventory::Error::TargetLayout => Self::TargetLayout,
            inventory::Error::Representation => Self::Representation,
        }
    }
}

fn work_overflow(control: &dyn Control) -> Failure {
    if let Err(failure) = control
        .check()
        .and_then(|()| control.charge_work(usize::MAX))
    {
        return failure;
    }
    Failure::new(
        FailureReason::LimitExceeded,
        "preparation-sdk-work-units",
        "pattern translation work exceeds its allowance",
    )
}
fn allocation_overflow(control: &dyn Control) -> Failure {
    if let Err(failure) = control.check() {
        return failure;
    }
    control.allocation_limit_reached();
    Failure::new(
        FailureReason::LimitExceeded,
        "preparation-sdk-reservation-bytes",
        "pattern translation allocation exceeds its allowance",
    )
}

/// `form` is the Reader's normalized representation. The same borrow supplies
/// guard, AST parser and translator; no independently supplied AST can replace it.
/// Reader's lower caller limit has already applied before this stage.
pub(super) fn prepare(form: &str, control: &dyn Control) -> Result<Hir, Error> {
    control.charge_work(entry_work().ok_or_else(|| work_overflow(control))?)?;
    control.check()?;
    // Authored groups preserve their nesting. Boundary/empty-set lowering can
    // add one atom-local group; it cannot synthesize recursively nested groups.
    let normalized_groups = crate::pattern::MAX_GROUP_DEPTH
        .checked_add(1)
        .ok_or_else(|| work_overflow(control))?;
    let parsed = ast::prepare(form, normalized_groups, AST_REPRESENTATION_DEPTH, control)?;
    let owned = inventory::inspect(parsed, control)?;
    if owned.summary.maximum_prefix_calls > PREFIX_CALLS
        || owned.summary.equality_height > EQUALITY_HEIGHT
    {
        control.check()?;
        return Err(Error::TranslationNesting);
    }
    // The pinned closed grammar excludes translator errors, but the defensive
    // error branch still prepays its sole owned source-string copy. No error is
    // formatted or retained. AST cleanup was prepaid by the earlier stage.
    let work = owned
        .work
        .checked_add(form.len())
        .ok_or_else(|| work_overflow(control))?;
    let allocation = owned
        .allocation
        .checked_add(form.len())
        .ok_or_else(|| allocation_overflow(control))?;
    control.charge_work(work)?;
    control.charge_allocation(allocation)?;
    control.check()?;
    let translated = {
        #[cfg(test)]
        tests::entered_translator();
        let mut translator = TranslatorBuilder::new().utf8(true).build();
        translator.translate(form, &owned.parsed.tree)
    };
    let output = match translated {
        Ok(tree) => Ok(tree),
        Err(error) => {
            drop(error);
            Err(Error::Representation)
        }
    };
    // No translator, AST or diagnostic owner survives the completion check.
    drop(owned);
    control.check()?;
    output
}

#[cfg(test)]
mod tests;
