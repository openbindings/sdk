//! Only publication relationships which can be established from the two documents
//! and report. No traversal of opaque kind data pretends to establish fidelity.
use crate::{
    PullRequest, PullResult, PullValidationError as Failure, RelationshipError as Rule,
    budget::Attempt,
};
use openbindings::JsonRef;
use std::collections::BTreeMap;

type Map<'a> = BTreeMap<&'a str, JsonRef<'a>>;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Section {
    Operations,
    Schemas,
    Bindings,
    Sources,
}
type Labels<'a> = BTreeMap<(Section, &'a str), ()>;

fn key(value: JsonRef<'_>) -> Result<&str, Failure> {
    value.as_str().ok_or_else(|| Rule::Identifier.into())
}
fn member<'a>(value: JsonRef<'a>, field: &str) -> JsonRef<'a> {
    value.get(field).expect("admitted carrier field")
}
fn index<'a>(value: Option<JsonRef<'a>>, attempt: &mut Attempt<'_>) -> Result<Map<'a>, Failure> {
    let mut map = Map::new();
    if let Some(value) = value {
        for entry in value.members().expect("admitted map") {
            let name = key(entry.name)?;
            attempt.map_insert::<&str, JsonRef<'_>>(map.len(), name.len())?;
            map.insert(name, entry.value);
        }
    }
    Ok(map)
}
fn lookup<'a>(
    map: &Map<'a>,
    name: &str,
    attempt: &mut Attempt<'_>,
) -> Result<Option<JsonRef<'a>>, Failure> {
    attempt.search(map.len(), name.len())?;
    Ok(map.get(name).copied())
}
fn contains(
    labels: &Labels<'_>,
    section: Section,
    key: &str,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.search(labels.len(), key.len().saturating_add(1))?;
    Ok(labels.contains_key(&(section, key)))
}
fn retain<'a>(
    labels: &mut Labels<'a>,
    section: Section,
    name: &'a str,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.map_insert::<(Section, &str), ()>(labels.len(), name.len().saturating_add(1))?;
    Ok(labels.insert((section, name), ()).is_none())
}

struct Additions<'a> {
    operations: Map<'a>,
    schemas: Map<'a>,
    bindings: Map<'a>,
}
impl<'a> Additions<'a> {
    fn uses(
        &self,
        value: Option<JsonRef<'a>>,
        own_binding: Option<&str>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        let mut uses = Labels::new();
        if let Some(value) = value {
            for label in value.elements().expect("admitted uses") {
                let label = key(label)?;
                attempt.work(label.len().saturating_add(1))?;
                let (map_name, name) = label.split_once('/').ok_or(Rule::Uses)?;
                let (section, map) = match map_name {
                    "operations" => (Section::Operations, &self.operations),
                    "schemas" => (Section::Schemas, &self.schemas),
                    "bindings" if Some(name) != own_binding => (Section::Bindings, &self.bindings),
                    _ => return Err(Rule::Uses.into()),
                };
                if lookup(map, name, attempt)?.is_none()
                    || !retain(&mut uses, section, name, attempt)?
                {
                    return Err(Rule::Uses.into());
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn check(
    request: &PullRequest,
    result: &PullResult,
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    let original = request.interface().value();
    let operations = index(original.get("operations"), attempt)?;
    let schemas = index(original.get("schemas"), attempt)?;
    let bindings = index(original.get("bindings"), attempt)?;
    let raw = result.value().value().view();
    let additions = member(raw, "additions");
    let additions = Additions {
        operations: index(additions.get("operations"), attempt)?,
        schemas: index(additions.get("schemas"), attempt)?,
        bindings: index(additions.get("bindings"), attempt)?,
    };
    // Original aliases also reserve names against newly added operation keys.
    // Added aliases and their mutual collisions belong to prospective core validation.
    let mut aliases = Labels::new();
    for operation in operations.values() {
        attempt.work(1)?;
        if let Some(values) = operation.get("aliases") {
            for alias in values.elements().expect("conformant aliases") {
                retain(&mut aliases, Section::Operations, key(alias)?, attempt)?;
            }
        }
    }
    for (new, old) in [
        (&additions.operations, &operations),
        (&additions.schemas, &schemas),
        (&additions.bindings, &bindings),
    ] {
        for name in new.keys() {
            if lookup(old, name, attempt)?.is_some() {
                return Err(Rule::KeyCollision.into());
            }
        }
    }
    for name in additions.operations.keys() {
        if contains(&aliases, Section::Operations, name, attempt)? {
            return Err(Rule::KeyCollision.into());
        }
    }
    for binding in additions.bindings.values() {
        let source = key(member(*binding, "source"))?;
        attempt.work(source.len().saturating_add(1))?;
        if source != request.source() {
            return Err(Rule::BindingSource.into());
        }
    }
    let report = member(raw, "report");
    let mut added = BTreeMap::new();
    for entry in member(report, "added").elements().expect("admitted added") {
        let name = key(member(entry, "binding"))?;
        let operation = key(member(entry, "operation"))?;
        let binding = lookup(&additions.bindings, name, attempt)?.ok_or(Rule::AddedCoverage)?;
        attempt.work(operation.len().saturating_add(1))?;
        if key(member(binding, "operation"))? != operation {
            return Err(Rule::BindingOperation.into());
        }
        let reused = entry.get("reused").is_some();
        let operation_map = if reused {
            &operations
        } else {
            &additions.operations
        };
        if lookup(operation_map, operation, attempt)?.is_none() {
            return Err(Rule::Reuse.into());
        }
        additions.uses(entry.get("uses"), Some(name), attempt)?;
        attempt.map_insert::<&str, ()>(added.len(), name.len())?;
        if added.insert(name, ()).is_some() {
            return Err(Rule::AddedCoverage.into());
        }
    }
    if added.len() != additions.bindings.len() {
        return Err(Rule::AddedCoverage.into());
    }
    let mut existing = Labels::new();
    if let Some(entries) = report.get("existing") {
        for entry in entries.elements().expect("admitted existing") {
            let (section, name) = if let Some(value) = entry.get("binding") {
                let name = key(value)?;
                let binding = lookup(&bindings, name, attempt)?.ok_or(Rule::ExistingTarget)?;
                let source = key(member(binding, "source"))?;
                attempt.work(source.len().saturating_add(1))?;
                if source != request.source() {
                    return Err(Rule::ExistingTarget.into());
                }
                (Section::Bindings, name)
            } else if let Some(value) = entry.get("schema") {
                let name = key(value)?;
                if lookup(&schemas, name, attempt)?.is_none() {
                    return Err(Rule::ExistingTarget.into());
                }
                (Section::Schemas, name)
            } else {
                let name = key(member(entry, "source"))?;
                attempt.work(name.len().saturating_add(1))?;
                if name != request.source() {
                    return Err(Rule::ExistingTarget.into());
                }
                (Section::Sources, name)
            };
            if !retain(&mut existing, section, name, attempt)? {
                return Err(Rule::DuplicateTarget.into());
            }
            if section == Section::Bindings
                && let Some(proposal) = entry.get("proposal")
            {
                additions.uses(proposal.get("uses"), Some(name), attempt)?;
            }
        }
    }
    for (name, binding) in &bindings {
        let source = key(member(*binding, "source"))?;
        attempt.work(source.len().saturating_add(1))?;
        if source == request.source() && !contains(&existing, Section::Bindings, name, attempt)? {
            return Err(Rule::ExistingCoverage.into());
        }
    }
    attempt.check()
}
