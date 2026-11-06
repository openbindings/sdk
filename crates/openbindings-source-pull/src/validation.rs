use crate::{
    PullLimits, PullRequest, PullResult, PullValidationError as Failure, budget::Attempt,
    relationships,
};
use openbindings::{ConformantDocument, Document, Edit, JsonRef, WorkControl};

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
    let additions = result
        .value()
        .value()
        .get("additions")
        .expect("admitted additions");
    insert(request, additions, None, attempt)
}

// Selection follows map/member order. None retains every map, including empty
// maps, as validate_for has always done. A subset creates only nonempty maps.
pub(crate) fn insert(
    request: &PullRequest,
    additions: JsonRef<'_>,
    selection: Option<&[bool]>,
    attempt: &mut Attempt<'_>,
) -> Result<ConformantDocument, Failure> {
    let root = request.interface().value();
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
    let mut offset = 0;
    for section in ["operations", "schemas", "bindings"] {
        if let Some(map) = additions.get(section) {
            let count = map.len().expect("map");
            let selected = selection.map(|bits| &bits[offset..offset + count]);
            offset += count;
            if selected.is_some_and(|bits| !bits.iter().any(|bit| *bit)) {
                continue;
            }
            if root.get(section).is_none() {
                let value = selected_map(map, selected, attempt)?;
                attempt.allocate(64)?;
                edits.push(Edit::set(&[section], value));
            } else {
                for (at, entry) in map.members().expect("admitted map").enumerate() {
                    if selected.is_some_and(|bits| !bits[at]) {
                        continue;
                    }
                    let name = entry.name.as_str().expect("relationship-admitted key");
                    edit_path(name, attempt)?;
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

fn edit_path(name: &str, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
    attempt.work(name.len().saturating_add(1))?;
    // String growth and pointer escaping can cumulatively allocate up to eight
    // bytes per name byte. Include the fixed section and edit path overhead.
    attempt.allocate(name.len().saturating_mul(8).saturating_add(128))
}

fn selected_map(
    map: JsonRef<'_>,
    selected: Option<&[bool]>,
    attempt: &mut Attempt<'_>,
) -> Result<openbindings::JsonValue, Failure> {
    let Some(bits) = selected.filter(|bits| bits.iter().any(|bit| !bit)) else {
        return Ok(map.to_owned());
    };
    let mut removals = attempt.vector(bits.iter().filter(|bit| !**bit).count())?;
    for (entry, keep) in map.members().expect("map").zip(bits) {
        if !keep {
            let name = entry.name.as_str().expect("relationship-admitted key");
            edit_path(name, attempt)?;
            removals.push(Edit::remove(&[name]));
        }
    }
    // Core builds a map from exact retained values without serializing them
    // through another JSON model. At most three such passes occur per keep.
    let selected = Document::from_json(map.to_owned()).edit(&removals);
    attempt.check()?;
    Ok(selected.map_err(Failure::Edit)?.value().to_owned())
}
