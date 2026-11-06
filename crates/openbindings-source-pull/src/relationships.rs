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
type Uses<'a> = BTreeMap<(Section, &'a str), ()>;
struct Added<'a> {
    operation: &'a str,
    reused: bool,
    uses: Uses<'a>,
}

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
    uses: &Uses<'_>,
    section: Section,
    key: &str,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.search(uses.len(), key.len().saturating_add(1))?;
    Ok(uses.contains_key(&(section, key)))
}
fn retain<'a>(
    uses: &mut Uses<'a>,
    section: Section,
    name: &'a str,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.map_insert::<(Section, &str), ()>(uses.len(), name.len().saturating_add(1))?;
    Ok(uses.insert((section, name), ()).is_none())
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
    ) -> Result<Uses<'a>, Failure> {
        let mut uses = Uses::new();
        if let Some(value) = value {
            for label in value.elements().expect("admitted uses") {
                let label = key(label)?;
                attempt.work(label.len().saturating_add(1))?;
                let (role, name) = label.split_once('/').ok_or(Rule::Uses)?;
                let (section, map) = match role {
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
        Ok(uses)
    }
}

/// A set is closed when every explicitly retained binding's declared uses and
/// implicit new operation are also retained. Checking each edge's complete set
/// suffices for transitive closure; no recursive expansion or cycle rejection.
fn closure(
    uses: &Uses<'_>,
    implicit_operation: Option<&str>,
    own_binding: Option<&str>,
    reports: &BTreeMap<&str, Added<'_>>,
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    for &(section, name) in uses.keys() {
        attempt.work(1)?;
        if section != Section::Bindings {
            continue;
        }
        attempt.search(reports.len(), name.len())?;
        let dependency = reports.get(name).ok_or(Rule::AddedCoverage)?;
        let retained = |section, key: &str, attempt: &mut Attempt<'_>| -> Result<bool, Failure> {
            attempt.work(key.len().saturating_add(1))?;
            Ok(
                (section == Section::Operations && Some(key) == implicit_operation)
                    || (section == Section::Bindings && Some(key) == own_binding)
                    || contains(uses, section, key, attempt)?,
            )
        };
        if !dependency.reused && !retained(Section::Operations, dependency.operation, attempt)? {
            return Err(Rule::UsesClosure.into());
        }
        for &(section, name) in dependency.uses.keys() {
            if !retained(section, name, attempt)? {
                return Err(Rule::UsesClosure.into());
            }
        }
    }
    Ok(())
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
    let mut aliases = Uses::new();
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
        let uses = additions.uses(entry.get("uses"), Some(name), attempt)?;
        attempt.map_insert::<&str, Added<'_>>(added.len(), name.len())?;
        if added
            .insert(
                name,
                Added {
                    operation,
                    reused,
                    uses,
                },
            )
            .is_some()
        {
            return Err(Rule::AddedCoverage.into());
        }
    }
    if added.len() != additions.bindings.len() {
        return Err(Rule::AddedCoverage.into());
    }
    for (binding, entry) in &added {
        closure(
            &entry.uses,
            (!entry.reused).then_some(entry.operation),
            Some(binding),
            &added,
            attempt,
        )?;
    }

    let mut existing = Uses::new();
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
            if matches!(section, Section::Bindings | Section::Schemas)
                && let Some(proposal) = entry.get("proposal")
            {
                let own_binding = (section == Section::Bindings).then_some(name);
                let uses = additions.uses(proposal.get("uses"), own_binding, attempt)?;
                closure(&uses, None, None, &added, attempt)?;
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
