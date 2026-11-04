//! Fresh AST ownership after allocation-free structural admission.
use super::{
    normalized::{self, Summary},
    profile,
};
use openbindings_internal_schema::{Control, Failure};
use regex_syntax::ast::{self, Ast};

#[derive(Debug)]
pub(super) struct Parsed {
    pub tree: Ast,
    pub inventory: Summary,
}
#[derive(Debug)]
pub(super) enum Refusal {
    Normalized(normalized::Refusal),
    ConstructionLimit,
    Representation,
}
#[derive(Debug)]
pub(super) enum Error {
    Refused(Refusal),
    Attempt(Failure),
}
impl From<Failure> for Error {
    fn from(failure: Failure) -> Self {
        Self::Attempt(failure)
    }
}

pub(super) fn prepare(
    form: &str,
    maximum_group_depth: usize,
    maximum_ast_depth: u32,
    control: &dyn Control,
) -> Result<Parsed, Error> {
    control.check()?;
    let inventory = normalized::inspect(
        form,
        maximum_group_depth.min(profile::NORMALIZED_GROUPS),
        control,
    )
    .map_err(|error| match error {
        normalized::Error::Refused(refusal) => Error::Refused(Refusal::Normalized(refusal)),
        normalized::Error::Attempt(failure) => Error::Attempt(failure),
    })?;
    if !profile::admits_normalized(&inventory) {
        control.check()?;
        return Err(Error::Refused(Refusal::ConstructionLimit));
    }
    control.check()?;
    let result = {
        #[cfg(test)]
        tests::entered_parser();
        let mut parser = ast::parse::ParserBuilder::new()
            .nest_limit(maximum_ast_depth.min(profile::AST_DEPTH))
            .build();
        parser.parse(form).map_err(|error| {
            drop(error);
            Error::Refused(Refusal::Representation)
        })
    };
    // Parser and dependency error owners are gone before the completion check.
    // A late shared failure also drops a successful tree before returning.
    control.check()?;
    result.map(|tree| Parsed { tree, inventory })
}

#[cfg(test)]
mod tests;
