//! Construction limits apply before either evaluator path is selected. A local
//! undefined operation cannot skip the accounting that a successful projection
//! would have performed. Inventory traversal stays on original known locations.
mod queries;
use super::*;
use openbindings_internal_schema::{child_shape, schema_index::SchemaSelector};
use std::collections::{BTreeSet, VecDeque};

pub(super) struct Checked {
    pub custom: Vec<usize>,
    pub resources: Vec<usize>,
    pub formats: HashMap<usize, formats::Format>,
    pub queries: PreparedQueries,
    pub patterns: PreparedPatterns,
    pub operands: HashMap<usize, operands::Prepared>,
    pub annotations: AnnotationDemand,
    pub leaves: HashSet<usize>,
}

/// Syntax admission traverses every declared schema position, including unused
/// definitions, content schemas and orphan conditionals. Evaluation reachability
/// below deliberately omits those positions, so it cannot certify this fact.
pub(super) fn meta_syntax(
    construction: &Construction<'_>,
    entry: usize,
) -> Result<Vec<usize>, PreparationFailure> {
    let space = construction.environment().space();
    let control = construction.control();
    control.charge_allocation(512)?;
    let mut queue = vec![entry];
    let mut custom = BTreeSet::new();
    while let Some(node) = queue.pop() {
        control.check()?;
        let original = space.node(node);
        if original.depth > 256 {
            return Err(limit("schema-depth-limit"));
        }
        space.check_resource(original.resource, node)?;
        if let Some(meta) = space.custom_meta(original.resource) {
            meta.map_err(super::meta::query_failure)?;
            control.charge_allocation(64)?;
            custom.insert(original.resource);
        }
        let profile = original.profile.ok_or_else(|| {
            super::meta::query_failure(QueryFailure::UnsupportedDialect(
                space.resource(original.resource).dialect.clone(),
            ))
        })?;
        if !profile.is_custom() {
            let valid = construction.check_standard(node)?;
            if !valid {
                return Err(PreparationFailure::new(
                    PreparationFailureKind::InvalidExternalSchema,
                ));
            }
        }
        let value = original.value.view();
        if profile.reference_object(value) {
            continue;
        }
        for field in value.members().into_iter().flatten() {
            control.charge_work(field.name.text().len().saturating_add(1))?;
            let Some(keyword) = field.name.as_str() else {
                continue;
            };
            let Some(selector) = profile.selector(keyword, field.value) else {
                continue;
            };
            control.charge_allocation(64)?;
            let children: Box<dyn Iterator<Item = JsonRef<'_>>> = match selector {
                SchemaSelector::Map | SchemaSelector::MapOrStringArray => {
                    Box::new(field.value.members().into_iter().flatten().map(|m| m.value))
                }
                SchemaSelector::Array => Box::new(field.value.elements().into_iter().flatten()),
                SchemaSelector::Direct => Box::new(std::iter::once(field.value)),
            };
            for child in children {
                control.check()?;
                if let Some(&child) = space
                    .source(original.source)
                    .positions
                    .get(&backend::ref_id(child))
                {
                    control.charge_allocation(32)?;
                    queue.push(child);
                }
            }
        }
    }
    control.charge_allocation(custom.len().saturating_mul(8))?;
    Ok(custom.into_iter().collect())
}

#[cfg(test)]
pub(super) fn check(
    environment: &SchemaEnvironment,
    limits: &Limits,
    control: &dyn openbindings_internal_schema::Control,
) -> Result<(), PreparationFailure> {
    let construction = Construction::new(environment.clone(), control)?;
    check_from(&construction, environment.entry(), limits).map(|_| ())
}

pub(super) fn check_from(
    construction: &Construction<'_>,
    entry: usize,
    limits: &Limits,
) -> Result<Checked, PreparationFailure> {
    let space = construction.environment().space();
    let control = construction.control();
    control.charge_allocation(512)?;
    let mut queue = VecDeque::from([entry]);
    let mut visited = HashSet::new();
    let mut resources = HashSet::new();
    let mut missing = HashSet::new();
    let mut dynamic = BTreeMap::<String, BTreeSet<usize>>::new();
    let mut used = BTreeSet::<String>::new();
    let mut patterns = Patterns::default();
    let mut readings = PreparedPatterns::new();
    let mut operands = operands::Preparation::default();
    let mut custom = Vec::new();
    let mut formats = HashMap::new();
    let mut queries = PreparedQueries::new();
    let mut shared_queries = queries::Queries::new(space);
    let mut annotations = AnnotationDemand::default();
    let mut leaves = HashSet::new();
    while let Some(node) = queue.pop_front() {
        control.check()?;
        if visited.contains(&node) {
            continue;
        }
        control.charge_allocation(1024)?;
        visited.insert(node);
        let original = space.node(node);
        if original.depth > 256 {
            return Err(limit("schema-depth-limit"));
        }
        if resources.insert(original.resource) {
            count_resources(resources.len(), missing.len(), limits)?;
            if let Some(meta) = space.custom_meta(original.resource) {
                meta.map_err(super::meta::query_failure)?;
                control.charge_allocation(32)?;
                custom.push(original.resource);
            }
            for (name, anchors) in &space.resource(original.resource).anchors {
                control.charge_work(name.len().saturating_add(1))?;
                for &(target, is_dynamic) in anchors {
                    control.check()?;
                    if is_dynamic {
                        control.charge_work(name.len().saturating_mul(8).saturating_add(1))?;
                        control
                            .charge_allocation(name.len().saturating_mul(2).saturating_add(256))?;
                    }
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
        let Some(profile) = original.profile else {
            continue;
        };
        if !profile.is_custom()
            && (profile.edition() != Dialect::Draft202012
                || space.resource(original.resource).origin
                    == openbindings_internal_schema::ResourceOrigin::Supplied)
        {
            let valid = construction.check_standard(node)?;
            if !valid {
                return Err(PreparationFailure::new(
                    PreparationFailureKind::InvalidExternalSchema,
                ));
            }
        }
        let schema = Interpreted {
            value: original.value.view(),
            profile,
        };
        operands.add(node, schema, control)?;
        annotations.properties |= schema.get("unevaluatedProperties").is_some();
        annotations.items |= schema.get("unevaluatedItems").is_some();
        let mut leaf = !["$ref", "$dynamicRef", "$recursiveRef"]
            .iter()
            .any(|keyword| schema.get(keyword).is_some());
        if profile.asserts_format()
            && let Some(format) = schema.get("format")
        {
            let format = formats::Format::prepare(format, control)?;
            control.charge_allocation(128)?;
            formats.insert(node, format);
        }
        if let Some(pattern) = schema.get("pattern")
            && let Some(source) = pattern.as_str()
        {
            let reading = patterns.add(source, false, limits, control)?;
            control.charge_allocation(512)?;
            readings.insert(backend::occurrence(pattern), reading);
        }
        if let Some(entries) = schema.get("patternProperties").and_then(|v| v.members()) {
            for entry in entries {
                control.check()?;
                if let Some(pattern) = entry.name.as_str() {
                    let reading = patterns.add(pattern, true, limits, control)?;
                    control.charge_allocation(512)?;
                    readings.insert(backend::occurrence(entry.name), reading);
                }
            }
        }
        if profile.edition() == Dialect::Draft201909
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
                // Each original occurrence retains its own keyword/effect
                // identity even when immutable static query results are shared.
                control.charge_allocation(1024)?;
                let query = shared_queries.query(node, reference, control)?;
                queries.insert((node, ReferenceKind::for_keyword(keyword)), query.clone());
                match query.as_ref() {
                    Ok(target) => {
                        queue.push_back(target.node);
                        if keyword == "$recursiveRef"
                            && reference == "#"
                            && space.node(target.node).profile
                                == Some(Profile::standard(Dialect::Draft201909))
                            && space
                                .node(target.node)
                                .value
                                .get("$recursiveAnchor")
                                .and_then(|v| v.as_bool())
                                == Some(true)
                            && use_name(&mut used, RECURSIVE_SCOPE, control)?
                        {
                            enqueue_dynamic(&mut queue, &dynamic, RECURSIVE_SCOPE, control)?;
                        }
                        if keyword == "$dynamicRef"
                            && let Some(name) = &target.dynamic_name
                            && use_name(&mut used, name, control)?
                        {
                            enqueue_dynamic(&mut queue, &dynamic, name, control)?;
                        }
                    }
                    Err(QueryFailure::ResourceUnavailable(uri)) => {
                        // A cache hit still clones/hashes the missing name and
                        // compares against this preparation's resource set.
                        control.charge_work(
                            uri.len()
                                .saturating_mul(missing.len().saturating_add(2))
                                .saturating_add(1),
                        )?;
                        control.charge_allocation(uri.len().saturating_add(32))?;
                        missing.insert(uri.clone());
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
                control.charge_work(field.name.text().len().saturating_add(1))?;
                let Some(keyword) = field.name.as_str() else {
                    continue;
                };
                // Presence disqualifies even empty or malformed applicators.
                // A permissive custom meta-schema can admit those operands;
                // finding no child positions does not make their behavior inert.
                if schema.get(keyword).is_some()
                    && (child_shape(keyword).is_some()
                        || matches!(keyword, "dependencies" | "additionalItems"))
                {
                    leaf = false;
                }
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
                let Some(selector) = profile.selector(keyword, field.value) else {
                    continue;
                };
                control.charge_allocation(64)?;
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
                    control.check()?;
                    if let Some(&child) = space
                        .source(original.source)
                        .positions
                        .get(&backend::ref_id(child))
                    {
                        control.charge_allocation(64)?;
                        queue.push_back(child);
                    }
                }
            }
        }
        if leaf {
            control.charge_allocation(128)?;
            leaves.insert(node);
        }
    }
    control.charge_allocation(resources.len().saturating_mul(8))?;
    let mut resources: Vec<_> = resources.into_iter().collect();
    resources.sort_unstable();
    Ok(Checked {
        custom,
        resources,
        formats,
        queries,
        patterns: readings,
        operands: operands.records,
        annotations,
        leaves,
    })
}
fn limit(resource: &str) -> PreparationFailure {
    PreparationFailure::new(PreparationFailureKind::Limit {
        resource: resource.into(),
    })
}
fn enqueue_dynamic(
    queue: &mut VecDeque<usize>,
    dynamic: &BTreeMap<String, BTreeSet<usize>>,
    name: &str,
    control: &dyn openbindings_internal_schema::Control,
) -> Result<(), PreparationFailure> {
    // Bound every possible key comparison before looking up candidates. This
    // work is separate from the used-name set and the static query cache.
    control.charge_work(name.len().saturating_mul(dynamic.len().saturating_add(1)))?;
    let candidates = dynamic.get(name);
    let count = candidates.map_or(0, BTreeSet::len);
    control.charge_work(count)?;
    control.charge_allocation(count.saturating_mul(16))?;
    queue.extend(candidates.into_iter().flatten().copied());
    Ok(())
}
// Reserve owned names and an upper bound on set comparisons on every use,
// including query-cache hits. Duplicate names still perform the lookup.
fn use_name(
    used: &mut BTreeSet<String>,
    name: &str,
    control: &dyn openbindings_internal_schema::Control,
) -> Result<bool, PreparationFailure> {
    control.charge_work(
        name.len()
            .saturating_mul(used.len().saturating_add(2))
            .saturating_add(1),
    )?;
    control.charge_allocation(name.len().saturating_add(256))?;
    Ok(used.insert(name.into()))
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
    forms: HashMap<String, Arc<crate::pattern::Reading>>,
    bytes: usize,
    matched: usize,
    compiled: usize,
}
impl Patterns {
    fn add(
        &mut self,
        source: &str,
        name: bool,
        limits: &Limits,
        control: &dyn openbindings_internal_schema::Control,
    ) -> Result<Arc<crate::pattern::Reading>, PreparationFailure> {
        control.check()?;
        if source.len() > limits.pattern_bytes {
            return Err(limit("pattern-byte-limit"));
        }
        // Debit the lexical preflight itself before scanning for expansions.
        control.charge_work(source.len().saturating_add(1))?;
        let (work, allocation) = super::super::pattern::preparation_cost(source)
            .ok_or_else(|| limit("preparation-sdk-reservation-bytes"))?;
        control.charge_work(work)?;
        control.charge_allocation(allocation)?;
        let reading = super::super::read_pattern(source, limits).map_err(|detail| {
            PreparationFailure::new(PreparationFailureKind::Limit {
                resource: detail.code.into_owned(),
            })
        })?;
        let super::super::pattern::Reading::Linear(linear) = &reading else {
            // Arc stores the full enum layout even for a small Unsupported
            // payload; include its header/alignment before allocating it.
            control.charge_allocation(std::mem::size_of::<crate::pattern::Reading>() + 64)?;
            return Ok(Arc::new(reading));
        };
        if !name && let Some(known) = self.forms.get(&linear.form) {
            return Ok(known.clone());
        }
        self.bytes = self.bytes.saturating_add(linear.form.len());
        if self.bytes > limits.total_pattern_bytes {
            return Err(limit("pattern-total-byte-limit"));
        }
        if let Some(simple) = &linear.simple {
            self.matched = self
                .matched
                .saturating_add(std::mem::size_of_val(simple))
                .saturating_add(simple.heap_bytes());
        } else {
            self.compiled = self.compiled.saturating_add(1);
        }
        if super::super::pattern::Memory::most_patterns(limits.total_pattern_memory, self.matched)
            .is_none_or(|capacity| self.compiled > capacity)
        {
            return Err(limit("pattern-total-memory-limit"));
        }
        // The reader's reservation covers the retained form clone. Reserve
        // the Arc and growing form index before constructing either. Pattern
        // values with the same normalized form share the reading counted by
        // the total-pattern policy; name operands are counted individually.
        control.charge_allocation(512)?;
        let reading = Arc::new(reading);
        if !name && let crate::pattern::Reading::Linear(linear) = reading.as_ref() {
            self.forms.insert(linear.form.clone(), reading.clone());
        }
        Ok(reading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openbindings_internal_schema::preparation::Attempt;
    #[test]
    fn short_unicode_properties_reserve_their_expansion_before_reading() {
        let failed = Attempt::run(1_000_000, 2048, &|| false, |a| {
            Patterns::default().add(r"\p{L}", false, &Limits::default(), a)
        })
        .unwrap_err();
        assert_eq!(failed.code, "preparation-sdk-reservation-bytes");
        let (reserved, retained) = Attempt::run(1_000_000, 8 * 1024 * 1024, &|| false, |a| {
            let mut patterns = Patterns::default();
            patterns
                .add(r"\p{L}", false, &Limits::default(), a)
                .unwrap();
            (
                a.allocated(),
                patterns.forms.keys().map(String::len).sum::<usize>(),
            )
        })
        .unwrap();
        assert!(retained > 10_000);
        assert!(reserved > retained * 4);
    }

    #[test]
    fn unresolved_reference_names_are_reserved_before_query_and_retention() {
        let uri = format!("https://example.test/{}", "n".repeat(100_000));
        let document = Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{{"$ref":"{uri}"}}}}}}"#)).unwrap().validate().unwrap().conformant().unwrap();
        let schema = document.schema("entry").unwrap();
        let environment = schema
            .environment(
                &ExternalSchemas::default(),
                &PreparationLimits::default(),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        let failed = Attempt::run(10_000_000, 2048, &|| false, |a| {
            check(&environment, &Limits::default(), a)
        })
        .unwrap_err();
        assert_eq!(failed.code, "preparation-sdk-reservation-bytes");
        assert!(
            Attempt::run(10_000_000, 8 * 1024 * 1024, &|| false, |a| {
                check(&environment, &Limits::default(), a)
            })
            .unwrap()
            .is_ok()
        );
    }

    #[test]
    fn literal_ranges_are_charged_again_for_repeated_shorthand_unions() {
        let literals: String = (0..1000)
            .map(|i| char::from_u32(0x1000 + i * 2).unwrap())
            .collect();
        let source = format!("[{literals}{}]", r"\s".repeat(1000));
        let failed = Attempt::run(usize::MAX, 1024 * 1024, &|| false, |a| {
            Patterns::default().add(&source, false, &Limits::default(), a)
        })
        .unwrap_err();
        assert_eq!(failed.code, "preparation-sdk-reservation-bytes");
        let (_, allocation) = super::super::super::pattern::preparation_cost(&source).unwrap();
        assert!(allocation > 16_482_598);
    }
}
