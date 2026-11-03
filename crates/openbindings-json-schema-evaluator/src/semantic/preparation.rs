//! Construction limits apply before either evaluator path is selected. A local
//! undefined operation cannot skip the accounting that a successful projection
//! would have performed. Inventory traversal stays on original known locations.
use super::*;
use openbindings_internal_schema::{child_shape, schema_index::SchemaSelector};
use std::collections::{BTreeSet, VecDeque};

pub(super) fn check(
    environment: &SchemaEnvironment,
    limits: &Limits,
    control: &dyn WorkControl,
) -> Result<(), PreparationFailure> {
    let space = environment.space();
    let mut queue = VecDeque::from([environment.entry()]);
    let mut visited = HashSet::new();
    let mut resources = HashSet::new();
    let mut missing = HashSet::new();
    let mut dynamic = BTreeMap::<String, BTreeSet<usize>>::new();
    let mut used = BTreeSet::<String>::new();
    let mut patterns = Patterns::default();
    while let Some(node) = queue.pop_front() {
        control.check()?;
        if !visited.insert(node) {
            continue;
        }
        let original = space.node(node);
        if original.depth > 256 {
            return Err(limit("schema-depth-limit"));
        }
        if resources.insert(original.resource) {
            count_resources(resources.len(), missing.len(), limits)?;
            for (name, anchors) in &space.resource(original.resource).anchors {
                for &(target, is_dynamic) in anchors {
                    if is_dynamic
                        && dynamic.entry(name.clone()).or_default().insert(target)
                        && used.contains(name)
                    {
                        queue.push_back(target);
                    }
                }
            }
        }
        if !space.eligible(original.resource) || space.query_inventory(original.resource).is_err() {
            continue;
        }
        let Some(dialect) = original.dialect else {
            continue;
        };
        if dialect != Dialect::Draft202012 {
            let valid = openbindings_internal_schema::fixed_schema::dialect_validity(
                &original.value,
                dialect,
            )
            .map_err(|_| limit("foreign-meta-schema-check"))?;
            if !valid {
                return Err(PreparationFailure::new(
                    PreparationFailureKind::InvalidExternalSchema,
                ));
            }
        }
        let schema = Interpreted {
            value: original.value.view(),
            dialect,
        };
        if let Some(pattern) = schema.get("pattern").and_then(|v| v.as_str()) {
            patterns.add(pattern, false, limits)?;
        }
        if let Some(entries) = schema.get("patternProperties").and_then(|v| v.members()) {
            for entry in entries {
                control.check()?;
                if let Some(pattern) = entry.name.as_str() {
                    patterns.add(pattern, true, limits)?;
                }
            }
        }
        if dialect == Dialect::Draft201909
            && schema.get("$recursiveAnchor").and_then(|v| v.as_bool()) == Some(true)
            && let Some(root) = space.resource(original.resource).root
            && dynamic
                .entry(RECURSIVE_SCOPE.into())
                .or_default()
                .insert(root)
            && used.contains(RECURSIVE_SCOPE)
        {
            queue.push_back(root);
        }
        for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            if let Some(reference) = schema.get(keyword).and_then(|v| v.as_str()) {
                match space.query(node, reference) {
                    Ok(target) => {
                        queue.push_back(target.node);
                        if keyword == "$recursiveRef"
                            && reference == "#"
                            && space.node(target.node).dialect == Some(Dialect::Draft201909)
                            && space
                                .node(target.node)
                                .value
                                .get("$recursiveAnchor")
                                .and_then(|v| v.as_bool())
                                == Some(true)
                            && used.insert(RECURSIVE_SCOPE.into())
                        {
                            queue.extend(
                                dynamic.get(RECURSIVE_SCOPE).into_iter().flatten().copied(),
                            );
                        }
                        if keyword == "$dynamicRef"
                            && let Some(name) = target.dynamic_name
                            && used.insert(name.clone())
                        {
                            queue.extend(dynamic.get(&name).into_iter().flatten().copied());
                        }
                    }
                    Err(QueryFailure::ResourceUnavailable(uri)) => {
                        missing.insert(uri);
                        count_resources(resources.len(), missing.len(), limits)?;
                    }
                    Err(QueryFailure::IdentityConflict) => {
                        let mut failure =
                            PreparationFailure::new(PreparationFailureKind::InvalidExternalSchema);
                        let bytes = space
                            .source(original.source)
                            .uri
                            .as_ref()
                            .map_or(0, String::len);
                        if let Some(remaining) = (64usize * 1024).checked_sub(bytes)
                            && backend::location_pointer_size(original.value.view(), remaining)
                                .is_some()
                        {
                            let at = space.location(node);
                            failure.origin = Some(SchemaOrigin::new(at.resource, at.pointer));
                        }
                        return Err(failure);
                    }
                    // A defect has no invented target. Its per-value effect is
                    // analyzed later; other original branches are still counted.
                    Err(_) => {}
                }
            }
        }
        if let Some(fields) = schema.value.members() {
            for field in fields {
                let Some(keyword) = field.name.as_str() else {
                    continue;
                };
                if schema.get(keyword).is_none()
                    || (child_shape(keyword).is_none()
                        && !matches!(keyword, "dependencies" | "additionalItems"))
                    || (matches!(keyword, "then" | "else") && schema.get("if").is_none())
                    || (keyword == "additionalItems"
                        && !schema
                            .get("items")
                            .is_some_and(|v| v.kind() == JsonKind::Array))
                {
                    continue;
                }
                let Some(selector) = dialect.selector(keyword, field.value) else {
                    continue;
                };
                let children: Box<dyn Iterator<Item = JsonRef<'_>>> = match selector {
                    SchemaSelector::Map | SchemaSelector::MapOrStringArray => {
                        Box::new(field.value.members().into_iter().flatten().map(|m| m.value))
                    }
                    SchemaSelector::Array => Box::new(field.value.elements().into_iter().flatten()),
                    SchemaSelector::Direct => Box::new(std::iter::once(field.value)),
                };
                for child in
                    children.filter(|v| matches!(v.kind(), JsonKind::Object | JsonKind::Boolean))
                {
                    if let Some(&child) = space
                        .source(original.source)
                        .positions
                        .get(&backend::ref_id(child))
                    {
                        queue.push_back(child);
                    }
                }
            }
        }
    }
    Ok(())
}
fn limit(resource: &str) -> PreparationFailure {
    PreparationFailure::new(PreparationFailureKind::Limit {
        resource: resource.into(),
    })
}
fn count_resources(
    known: usize,
    missing: usize,
    limits: &Limits,
) -> Result<(), PreparationFailure> {
    if limits
        .schema_resources
        .is_some_and(|cap| known.saturating_add(missing) > cap)
    {
        Err(limit("schema-resource-limit"))
    } else {
        Ok(())
    }
}
#[derive(Default)]
struct Patterns {
    forms: HashSet<String>,
    bytes: usize,
    matched: usize,
    compiled: usize,
}
impl Patterns {
    fn add(&mut self, source: &str, name: bool, limits: &Limits) -> Result<(), PreparationFailure> {
        let reading = super::super::read_pattern(source, limits).map_err(|detail| {
            PreparationFailure::new(PreparationFailureKind::Limit {
                resource: detail.code,
            })
        })?;
        let super::super::pattern::Reading::Linear(linear) = reading else {
            return Ok(());
        };
        if !name && !self.forms.insert(linear.form.clone()) {
            return Ok(());
        }
        self.bytes = self.bytes.saturating_add(linear.form.len());
        if self.bytes > limits.total_pattern_bytes {
            return Err(limit("pattern-total-byte-limit"));
        }
        if let Some(simple) = linear.simple {
            self.matched = self
                .matched
                .saturating_add(std::mem::size_of_val(&simple))
                .saturating_add(simple.heap_bytes());
        } else {
            self.compiled = self.compiled.saturating_add(1);
        }
        if super::super::pattern::Memory::most_patterns(limits.total_pattern_memory, self.matched)
            .is_none_or(|capacity| self.compiled > capacity)
        {
            return Err(limit("pattern-total-memory-limit"));
        }
        Ok(())
    }
}
