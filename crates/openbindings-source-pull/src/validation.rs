use crate::{
    PullLimits, PullRequest, PullResult, PullValidationError as Failure, budget::Attempt,
    relationships,
};
use openbindings::{ConformantDocument, Edit, WorkControl};

impl PullResult {
    /// Check request/result relationships and validate a new snapshot containing
    /// all additions. The original is immutable; stale proposals are not applied.
    /// This proof covers core conformance only, not a kind's fidelity, reuse,
    /// change classifications, binding data needs or exhaustiveness claims.
    ///
    /// The returned document retains the original JSON admission limits. The
    /// core edit and validation passes stay within the document limits and do not
    /// spend the relationship ledger. Cancellation is checked between passes.
    pub fn validate_for(
        &self,
        request: &PullRequest,
        limits: &PullLimits,
        control: &dyn WorkControl,
    ) -> Result<ConformantDocument, Failure> {
        let mut attempt = Attempt::new(*limits, control);
        attempt.request(request)?;
        validate(self, request, &mut attempt)
    }
}

pub(crate) fn validate(
    result: &PullResult,
    request: &PullRequest,
    attempt: &mut Attempt<'_>,
) -> Result<ConformantDocument, Failure> {
    attempt.response(result.value().value().text().len())?;
    relationships::check(request, result, attempt)?;
    let root = request.interface().value();
    let additions = result
        .value()
        .value()
        .get("additions")
        .expect("admitted additions");
    // Reserve exactly the maximum edit count before assembling any owned paths.
    let mut count = 0usize;
    for section in ["operations", "schemas", "bindings"] {
        if let Some(map) = additions.get(section) {
            count = count
                .checked_add(if root.get(section).is_some() {
                    map.len().expect("map")
                } else {
                    1
                })
                .ok_or(Failure::LimitExceeded("relationship-allocation-bytes"))?;
        }
    }
    let mut edits = attempt.vector::<Edit>(count)?;
    for section in ["operations", "schemas", "bindings"] {
        if let Some(map) = additions.get(section) {
            if root.get(section).is_none() {
                attempt.allocate(64)?;
                edits.push(Edit::set(&[section], map.to_owned()));
            } else {
                for entry in map.members().expect("admitted map") {
                    let name = entry.name.as_str().expect("relationship-admitted key");
                    attempt.work(name.len().saturating_add(1))?;
                    // Edit::set grows a String. Final capacity can approach twice
                    // the required bytes; cumulative allocations approach twice
                    // that capacity. Each name byte can need two escaped bytes.
                    attempt.allocate(name.len().saturating_mul(8).saturating_add(128))?;
                    edits.push(Edit::set(&[section, name], entry.value.to_owned()));
                }
            }
        }
    }
    attempt.check()?;
    if edits.is_empty() {
        return Ok(request.interface().clone());
    }
    let prospective = request.interface().document().edit(&edits);
    attempt.check()?;
    let prospective = prospective.map_err(Failure::Edit)?;
    let validation = prospective.validate();
    attempt.check()?;
    let validation = validation.map_err(Failure::Version)?;
    let prospective = validation
        .conformant()
        .ok_or(Failure::Prospective(validation))?;
    Ok(prospective)
}
