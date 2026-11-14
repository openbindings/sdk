//! Using a pull result as application code, the steps of the source-puller
//! interface's "Using the result". To take everything a pull offers, call
//! `PullResult::validate_for`; these functions are the other common cases.
//!
//! The walk in [`keep_part`] sets no work limits of its own, which suits a source
//! the application trusts. `validate_for` and the final validation still check
//! the result and the edited document.
// guide:prelude:start
use openbindings::{ConformantDocument, Edit, JsonValue, ReferenceResolution, WorkControl};
use openbindings_source_pull::{
    Change, ExistingEntry, ExistingTarget, Proposal, PullLimits, PullRequest, PullResult,
};
use std::collections::{BTreeMap, BTreeSet};

/// What these application functions return on failure.
pub type Error = Box<dyn std::error::Error>;
// guide:prelude:end

// guide:proposals:start
/// One proposal as document edits: what it carries, copied onto the existing
/// entry, and what its `remove` names. Unknown proposal metadata is not copied.
pub fn proposal_edits(request: &PullRequest, entry: &ExistingEntry) -> Vec<Edit> {
    proposal_writes(request, entry)
        .into_iter()
        .map(|(path, value)| {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            match value {
                Some(value) => Edit::set(&path, value),
                None => Edit::remove(&path),
            }
        })
        .collect()
}

fn proposal_writes(
    request: &PullRequest,
    entry: &ExistingEntry,
) -> Vec<(Vec<String>, Option<JsonValue>)> {
    let mut writes = Vec::new();
    let mut write = |path: &[&str], value| {
        writes.push((path.iter().map(|part| (*part).to_owned()).collect(), value));
    };
    match (entry.target(), entry.proposal()) {
        (ExistingTarget::Binding(key), Some(Proposal::Binding(proposal))) => {
            let key = key.as_str().unwrap();
            let binding = request.interface().binding(key).unwrap();
            let operation = binding.operation_key();
            if let Some(data) = proposal.data() {
                write(&["bindings", key, "data"], Some(data));
            }
            if let Some(input) = proposal.input() {
                write(&["operations", operation, "input"], Some(input));
            }
            if let Some(output) = proposal.output() {
                write(&["operations", operation, "output"], Some(output));
            }
            for field in proposal.remove().into_iter().flatten() {
                match field.as_str().unwrap() {
                    "data" => write(&["bindings", key, "data"], None),
                    field => write(&["operations", operation, field], None),
                }
            }
        }
        (ExistingTarget::Schema(key), Some(Proposal::Schema(proposal))) => {
            write(&["schemas", key.as_str().unwrap()], Some(proposal.schema()));
        }
        (ExistingTarget::Source(key), Some(Proposal::Source(proposal))) => {
            let key = key.as_str().unwrap();
            write(&["sources", key, "data"], proposal.data());
        }
        _ => {}
    }
    writes
}
// guide:proposals:end

// guide:updates:start
/// Take everything the pull offers, then every non-breaking update. Every
/// addition is present, so what a proposal needs is already there.
/// Agreeing writes share one edit; contradictory writes require a caller decision.
pub fn take_non_breaking_updates(
    result: &PullResult,
    request: &PullRequest,
    limits: &PullLimits,
    control: &dyn WorkControl,
) -> Result<ConformantDocument, Error> {
    let everything = result.validate_for(request, limits, control)?;
    let mut writes: BTreeMap<Vec<String>, Option<JsonValue>> = BTreeMap::new();
    for (path, value) in result
        .report()
        .existing()
        .into_iter()
        .flatten()
        .filter(|entry| entry.change() == Some(Change::NonBreaking))
        .flat_map(|entry| proposal_writes(request, &entry))
    {
        if let Some(previous) = writes.get(&path) {
            let agrees = match (previous, &value) {
                (None, None) => true,
                (Some(previous), Some(value)) => previous.semantic_eq(value) == Some(true),
                _ => false,
            };
            if !agrees {
                return Err(
                    "non-breaking proposals write different values to the same slot".into(),
                );
            }
        } else {
            writes.insert(path, value);
        }
    }
    let edits: Vec<Edit> = writes
        .into_iter()
        .map(|(path, value)| {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            match value {
                Some(value) => Edit::set(&path, value),
                None => Edit::remove(&path),
            }
        })
        .collect();
    let edited = everything.document().edit(&edits)?.validate()?;
    edited
        .conformant()
        .ok_or_else(|| "the edited document does not conform".into())
}
// guide:updates:end

// guide:partial:start
/// Keep chosen added bindings and what they reach.
pub fn keep_part(
    result: &PullResult,
    request: &PullRequest,
    bindings: &[&str],
    limits: &PullLimits,
    control: &dyn WorkControl,
) -> Result<ConformantDocument, Error> {
    // References resolve in the document holding every addition.
    let everything = result.validate_for(request, limits, control)?;
    let references = everything.document().references();
    if !references.complete {
        return Err("reference inspection stopped early".into());
    }

    let additions = result.additions();
    let is_addition = |pointer: &str| additions.value().value().at(pointer).is_some();
    let mut wanted: Vec<String> = bindings
        .iter()
        .map(|key| format!("/bindings/{key}"))
        .collect();
    if wanted.iter().any(|pointer| !is_addition(pointer)) {
        return Err("a selected binding is not an addition".into());
    }

    let mut seen = BTreeSet::new();
    while let Some(pointer) = wanted.pop() {
        // A reference into an addition keeps the whole entry.
        let entry = pointer.split('/').take(3).collect::<Vec<_>>().join("/");
        let root = if is_addition(&entry) { entry } else { pointer };
        if !seen.insert(root.clone()) {
            continue;
        }
        // An added binding needs its added operation and what its `uses` names.
        if let Some(key) = root.strip_prefix("/bindings/")
            && let Some(added) = result.report().added().find(|added| added.binding() == key)
        {
            if !added.reused() {
                wanted.push(format!(
                    "/operations/{}",
                    added.operation().as_str().unwrap()
                ));
            }
            for label in added.uses().into_iter().flatten() {
                wanted.push(format!("/{}", label.as_str().unwrap()));
            }
        }
        // Everything this entry refers to, through additions and existing entries.
        let inside = format!("{root}/");
        for reference in &references.references {
            let from = reference.location.pointer.as_str().unwrap_or_default();
            if let (true, ReferenceResolution::Located { target, .. }) =
                (from.starts_with(&inside), &reference.resolution)
                && target.source_uri.is_none()
            {
                wanted.push(target.pointer.as_str().unwrap().to_owned());
            }
        }
    }

    // `Edit::set` needs its parent, and one batch cannot create a map and fill it.
    let mut document = request.interface().document().clone();
    for map in ["schemas", "bindings"] {
        let prefix = format!("/{map}/");
        let needed = seen
            .iter()
            .any(|pointer| pointer.starts_with(&prefix) && is_addition(pointer));
        if needed && document.value().get(map).is_none() {
            document = document.edit(&[Edit::set(&[map], JsonValue::parse("{}")?)])?;
        }
    }
    let edits: Vec<Edit> = seen
        .iter()
        .filter_map(|pointer| Some((pointer, additions.value().value().at(pointer)?)))
        .map(|(pointer, value)| Edit::set_at(pointer.as_str(), value.to_owned()))
        .collect();
    if edits.is_empty() {
        return Ok(request.interface().clone());
    }
    let edited = document.edit(&edits)?.validate()?;
    edited
        .conformant()
        .ok_or_else(|| "the kept document does not conform".into())
}
// guide:partial:end
