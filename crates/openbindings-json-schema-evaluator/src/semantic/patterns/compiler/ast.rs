//! Fresh AST ownership after allocation-free structural admission.
use super::{normalized, profile};
use openbindings_internal_schema::{Control, Failure};
use regex_syntax::ast::{self, Ast};

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
impl From<profile::AdmissionError> for Error {
    fn from(error: profile::AdmissionError) -> Self {
        match error {
            profile::AdmissionError::PatternLimit => Self::Refused(Refusal::ConstructionLimit),
            profile::AdmissionError::Attempt(failure) => Self::Attempt(failure),
        }
    }
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
    budget: &profile::Budget,
    control: &dyn Control,
) -> Result<Ast, Error> {
    budget.check(control)?;
    let inventory = normalized::inspect(
        form,
        maximum_group_depth.min(profile::NORMALIZED_GROUPS),
        control,
    )
    .map_err(|error| match error {
        normalized::Error::Refused(refusal) => Error::Refused(Refusal::Normalized(refusal)),
        normalized::Error::Attempt(failure) => Error::Attempt(failure),
    })?;
    budget.ast(&inventory, control)?;
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
    budget.check(control)?;
    result
}

#[cfg(test)]
mod tests;
