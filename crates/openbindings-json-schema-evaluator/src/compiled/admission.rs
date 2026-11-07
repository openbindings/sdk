//! Pre-entry selection for the private restored compiler experiment.
use super::*;
use openbindings_internal_schema::{
    Control, OriginalEnvironment,
    dialect::{Dialect, Profile},
};
use std::collections::BTreeSet;

#[derive(Clone, Default)]
pub(super) struct Features {
    pub scratch: bool,
    pub integer: bool,
    pub literals: bool,
    pub noninteger_bound: bool,
    pub literal_bytes: usize,
    pub literal_nodes: usize,
    pub multiple: bool,
    pub multiplier: usize,
    pub unique: bool,
    pub advancing: bool,
    pub recursive: bool,
    pub recursive_allocation_free: bool,
    pub readings: BTreeMap<Arc<str>, pattern::Reading>,
}

#[derive(Default)]
struct ScalarLiterals {
    bytes: usize,
    nodes: usize,
}
impl ScalarLiterals {
    fn record(&mut self, literal: JsonRef<'_>) -> bool {
        if !matches!(
            literal.kind(),
            JsonKind::Null | JsonKind::Boolean | JsonKind::String
        ) {
            return false;
        }
        self.bytes = self.bytes.saturating_add(literal.text().len());
        self.nodes = self.nodes.saturating_add(1);
        true
    }
    fn restore(&self, features: &mut Features) {
        features.scratch |= self.nodes != 0;
        features.literals |= self.nodes != 0;
        features.literal_bytes = features.literal_bytes.saturating_add(self.bytes);
        features.literal_nodes = features.literal_nodes.saturating_add(self.nodes);
    }
}
impl Features {
    fn literal(&mut self, literal: JsonRef<'_>) {
        self.scratch = true;
        self.literals = true;
        self.literal_bytes = self.literal_bytes.saturating_add(literal.text().len());
        self.literal_nodes = self
            .literal_nodes
            .saturating_add(backend::node_count(literal));
    }
}

pub(super) struct Admitted {
    pub features: Features,
    pub reach: openbindings_internal_schema::Reach,
}
// The outer Result preserves actual preparation failure; this inner capability
// result chooses original evaluation without a backend attempt or heap wrapper.
pub(super) type Selection = Result<Admitted, &'static str>;

pub(super) fn select(
    plan: &crate::semantic::Plan,
    limits: &crate::Limits,
    preparation: &PreparationLimits,
    attempt: &dyn Control,
    reading: &dyn Fn(&str) -> Result<Option<crate::pattern::Shared>, PreparationFailure>,
) -> Result<Selection, PreparationFailure> {
    attempt.check().map_err(PreparationFailure::from)?;
    let default = crate::Limits::default();
    if limits.evaluation_sdk_work_units != default.evaluation_sdk_work_units
        || limits.evaluation_sdk_reservation_bytes != default.evaluation_sdk_reservation_bytes
        || limits.pattern_bytes != default.pattern_bytes
        || limits.pattern_depth != default.pattern_depth
        || limits.total_pattern_bytes != default.total_pattern_bytes
        || limits.total_pattern_memory != default.total_pattern_memory
        || limits.schema_resources != default.schema_resources
    {
        return Ok(Err("explicit-evaluator-allowance"));
    }
    let default = PreparationLimits::default();
    if preparation.max_schema_nodes != default.max_schema_nodes
        || preparation.max_identity_bytes != default.max_identity_bytes
        || preparation.max_sdk_work_units != default.max_sdk_work_units
        || preparation.max_sdk_reservation_bytes != default.max_sdk_reservation_bytes
    {
        return Ok(Err("explicit-preparation-allowance"));
    }
    if !plan.compiled_static_queries() {
        return Ok(Err("unsettled-or-dynamic-reference"));
    }
    if !plan.compiled_known_patterns() {
        return Ok(Err("local-pattern-effect-or-limit"));
    }
    let environment = plan.compiled_environment();
    let space = environment.space();
    // Cover selector sets/maps and the full application graph before creating
    // them. The immutable inventory is already owned by Plan.
    attempt
        .charge_allocation(space.node_count().saturating_mul(1024).saturating_add(512))
        .map_err(PreparationFailure::from)?;
    let Some(mut reach) = plan.compiled_reach(attempt)? else {
        return Ok(Err("incomplete-prepared-closure"));
    };
    let mut nodes: Vec<_> = reach.nodes.iter().copied().collect();
    nodes.sort_unstable();
    let mut own = HashMap::new();
    let mut scratch_own = HashMap::new();
    let mut scalar_literals = ScalarLiterals::default();
    let mut edges: HashMap<_, Vec<_>> = reach
        .edges
        .iter()
        .map(|(&from, children)| {
            (
                from,
                children
                    .iter()
                    .map(|&to| recursion::Edge {
                        to,
                        advances: false,
                    })
                    .collect(),
            )
        })
        .collect();
    let mut forms = BTreeSet::new();
    let mut accounted_sources = BTreeSet::new();
    let mut pattern_bytes = 0usize;
    let mut compiled_patterns = 0usize;
    let mut matched_bytes = 0usize;
    let mut features = Features::default();
    let mut source_bytes = 0usize;
    let mut recursive_scalar_only = true;
    let mut recursive_allocation_free = true;
    for &node in &nodes {
        attempt.check().map_err(PreparationFailure::from)?;
        let original = space.node(node);
        let Some(profile) = original.profile else {
            return Ok(Err("unknown-original-profile"));
        };
        if profile != Profile::standard(Dialect::Draft202012) {
            return Ok(Err("custom-or-foreign-profile"));
        }
        if !space.eligible(original.resource) || space.query_inventory(original.resource).is_err() {
            return Ok(Err("unestablished-original-identity"));
        }
        if backend::has_unpaired(&original.value) || original.value.has_duplicate_names() {
            return Ok(Err("exact-schema-text-or-duplicate-names"));
        }
        let value = original.value.view();
        source_bytes = source_bytes.saturating_add(original.value.bytes().len());
        if [
            "unevaluatedProperties",
            "unevaluatedItems",
            "$dynamicRef",
            "$recursiveRef",
        ]
        .iter()
        .any(|keyword| profile.get(value, keyword).is_some())
        {
            return Ok(Err("original-annotation-or-recursive-scope"));
        }
        let mut weight = 0usize;
        let scalar_start = scalar_literals.nodes;
        if let Some(types) = profile.get(value, "type") {
            let integer = types.as_str() == Some("integer")
                || types
                    .elements()
                    .is_some_and(|mut names| names.any(|n| n.as_str() == Some("integer")));
            features.scratch |= integer;
            features.integer |= integer;
            weight += usize::from(integer);
        }
        for keyword in openbindings_internal_json::numeric::KEYWORDS {
            if let Some(bound) = profile.get(value, keyword).and_then(|v| v.number_text()) {
                recursive_allocation_free = false;
                features.scratch = true;
                features.literal_bytes = features.literal_bytes.saturating_add(bound.len());
                features.literal_nodes = features.literal_nodes.saturating_add(1);
                features.noninteger_bound |= bound.parse::<i64>().is_err();
                weight = weight.saturating_add(1);
                if keyword == "multipleOf" {
                    features.multiple = true;
                    if !multiple_token(bound) || !inline_multiple_token(bound) {
                        return Ok(Err("multiple-of-arithmetic-domain"));
                    }
                }
            }
        }
        if let Some(literal) = profile.get(value, "const") {
            recursive_scalar_only &= !matches!(literal.kind(), JsonKind::Array | JsonKind::Object);
            recursive_allocation_free &= matches!(
                literal.kind(),
                JsonKind::Null | JsonKind::Boolean | JsonKind::String
            );
            if !scalar_literals.record(literal) {
                features.literal(literal);
            }
            weight = weight.saturating_add(1);
        }
        if let Some(choices) = profile.get(value, "enum").and_then(|v| v.elements()) {
            for literal in choices {
                recursive_scalar_only &=
                    !matches!(literal.kind(), JsonKind::Array | JsonKind::Object);
                recursive_allocation_free &= matches!(
                    literal.kind(),
                    JsonKind::Null | JsonKind::Boolean | JsonKind::String
                );
                attempt.check().map_err(PreparationFailure::from)?;
                if !scalar_literals.record(literal) {
                    features.literal(literal);
                }
                weight = weight.saturating_add(1);
            }
        }
        if profile.get(value, "uniqueItems").and_then(|v| v.as_bool()) == Some(true) {
            recursive_scalar_only = false;
            recursive_allocation_free = false;
            features.unique = true;
            features.scratch = true;
            features.literals = true;
            weight = weight.saturating_add(1);
        }
        own.insert(node, weight);
        scratch_own.insert(
            node,
            weight.saturating_sub(scalar_literals.nodes - scalar_start),
        );
        // Reuse resolved reference targets, and add original active child
        // applications. Keep same-instance edges for projection and every edge
        // for the cumulative scratch bound, including repeated child paths.
        for keyword in [
            "allOf",
            "anyOf",
            "oneOf",
            "not",
            "if",
            "then",
            "else",
            "dependentSchemas",
            "properties",
            "patternProperties",
            "additionalProperties",
            "propertyNames",
            "items",
            "prefixItems",
            "contains",
        ] {
            if matches!(keyword, "then" | "else") && profile.get(value, "if").is_none() {
                continue;
            }
            let Some(operand) = profile.get(value, keyword) else {
                continue;
            };
            let in_place = matches!(
                keyword,
                "allOf" | "anyOf" | "oneOf" | "not" | "if" | "then" | "else" | "dependentSchemas"
            );
            let children: Vec<_> = match keyword {
                "properties" | "patternProperties" | "dependentSchemas" => operand
                    .members()
                    .into_iter()
                    .flatten()
                    .map(|m| m.value)
                    .collect(),
                "prefixItems" | "allOf" | "anyOf" | "oneOf" => {
                    operand.elements().into_iter().flatten().collect()
                }
                _ => vec![operand],
            };
            for child in children {
                if let Some(&target) = space
                    .source(original.source)
                    .positions
                    .get(&backend::ref_id(child))
                {
                    if !reach.nodes.contains(&target) {
                        return Ok(Err("incomplete-prepared-closure"));
                    }
                    if in_place {
                        reach.edges.entry(node).or_default().push(target);
                    } else {
                        features.advancing = true;
                    }
                    edges.entry(node).or_default().push(recursion::Edge {
                        to: target,
                        advances: !in_place,
                    });
                }
            }
        }
        if let Some(pattern) = profile.get(value, "pattern").and_then(|v| v.as_str()) {
            if !read_pattern(pattern, attempt, &mut features.readings, reading, false)? {
                return Ok(Err("pattern-reader-capability"));
            }
            if accounted_sources.insert(pattern) {
                {
                    let linear = &features.readings[pattern];
                    attempt
                        .charge_work(linear.form.len().saturating_mul(2).saturating_add(1))
                        .map_err(PreparationFailure::from)?;
                    if !forms.contains(&linear.form) {
                        attempt
                            .charge_allocation(linear.form.len().saturating_add(128))
                            .map_err(PreparationFailure::from)?;
                        forms.insert(linear.form.clone());
                        pattern_bytes = pattern_bytes.saturating_add(linear.form.len());
                        if let Some(simple) = &linear.simple {
                            matched_bytes =
                                matched_bytes.saturating_add(operands::run_matcher_bytes(simple));
                        } else {
                            compiled_patterns += 1;
                        }
                    }
                }
            }
        }
        if let Some(patterns) = profile
            .get(value, "patternProperties")
            .and_then(|v| v.members())
        {
            let mut names = BTreeSet::new();
            for pattern in patterns {
                let Some(pattern) = pattern.name.as_str() else {
                    return Ok(Err("exact-pattern-name"));
                };
                if !read_pattern(pattern, attempt, &mut features.readings, reading, true)? {
                    return Ok(Err("pattern-name-reader-capability"));
                }
                {
                    let linear = &features.readings[pattern];
                    attempt
                        .charge_work(linear.form.len().saturating_add(1))
                        .map_err(PreparationFailure::from)?;
                    let height = names.len().checked_ilog2().unwrap_or(0) as usize + 1;
                    attempt
                        .charge_work(
                            linear
                                .form
                                .len()
                                .saturating_add(1)
                                .saturating_mul(12 * height),
                        )
                        .map_err(PreparationFailure::from)?;
                    if !names.insert(linear.form.clone()) {
                        return Ok(Err("normalized-property-name-alias"));
                    }
                    pattern_bytes = pattern_bytes.saturating_add(linear.form.len());
                    compiled_patterns += 1;
                }
            }
        }
        if pattern_bytes > limits.total_pattern_bytes
            || pattern::Memory::most_patterns(limits.total_pattern_memory, matched_bytes)
                .is_none_or(|most| compiled_patterns > most)
        {
            return Ok(Err("restored-private-pattern-profile"));
        }
    }
    // Acyclic paths retain their cumulative exact-scratch bound. Productive
    // recursion has a separate pre-entry qualification, never a constant
    // substituted for that cumulative bound.
    let mut weights = HashMap::<usize, usize>::new();
    let mut visiting = std::collections::HashSet::new();
    let mut recursive = false;
    'weights: for &root in &nodes {
        let mut pending = vec![(root, false)];
        while let Some((node, leaving)) = pending.pop() {
            attempt.check().map_err(PreparationFailure::from)?;
            if weights.contains_key(&node) {
                continue;
            }
            let edges = edges.get(&node).map(Vec::as_slice).unwrap_or(&[]);
            if leaving {
                visiting.remove(&node);
                let sum = edges
                    .iter()
                    .fold(*scratch_own.get(&node).unwrap_or(&0), |total, child| {
                        total.saturating_add(*weights.get(&child.to).unwrap_or(&0))
                    });
                weights.insert(node, sum);
                features.multiplier = features.multiplier.max(sum);
            } else {
                if !visiting.insert(node) {
                    recursive = true;
                    break 'weights;
                }
                pending.push((node, true));
                for child in edges {
                    if !weights.contains_key(&child.to) {
                        pending.push((child.to, false));
                    }
                }
            }
        }
    }
    if recursive {
        // Recursion qualification and its exact weight retain every original
        // literal charge. Restore those operands for its existing estimate too.
        // Only the acyclic fold discounts allocation-free scalar literals.
        scalar_literals.restore(&mut features);
        // Scalar exact predicates have one subject. Container equality and
        // uniqueness add internal value traversals outside this graph proof.
        if !recursive_scalar_only {
            return Ok(Err("recursive-exact-scratch-profile"));
        }
        let proof = match recursion::qualifies(
            space,
            environment.entry(),
            &nodes,
            &reach,
            &edges,
            &own,
            attempt,
        )? {
            Ok(proof) => proof,
            Err(reason) => return Ok(Err(reason)),
        };
        features.recursive = true;
        features.recursive_allocation_free = recursive_allocation_free;
        // Discard the partial acyclic fold; only the complete cut-DAG proof
        // supplies this multiplier, summed across distinct productive entries.
        features.multiplier = proof.exact;
    }
    features.multiplier = features.multiplier.max(1);
    // SDK conversion/index owners have their own coarse reservation. Opaque
    // dependency construction is admitted by the retained baseline profile and
    // measured separately; this is not its abandoned allocation certificate.
    attempt
        .charge_allocation(
            nodes
                .len()
                .saturating_mul(1024)
                .saturating_add(source_bytes.saturating_mul(8))
                .saturating_add(512),
        )
        .map_err(PreparationFailure::from)?;
    attempt.check().map_err(PreparationFailure::from)?;
    Ok(Ok(Admitted { features, reach }))
}

fn read_pattern(
    source: &str,
    attempt: &dyn Control,
    readings: &mut BTreeMap<Arc<str>, pattern::Reading>,
    reading: &dyn Fn(&str) -> Result<Option<crate::pattern::Shared>, PreparationFailure>,
    property_name: bool,
) -> Result<bool, PreparationFailure> {
    attempt
        .charge_work(source.len().saturating_add(1))
        .map_err(PreparationFailure::from)?;
    // Property names always enter the vendor Regex matcher, even when the
    // current Plan can represent that source with its Simple run matcher.
    if property_name && source.contains('{') {
        return Ok(false);
    }
    if readings.contains_key(source) {
        return Ok(true);
    }
    attempt
        .charge_allocation(source.len().saturating_add(512))
        .map_err(PreparationFailure::from)?;
    let Some(linear) = reading(source)? else {
        return Ok(false);
    };
    // Current PikeVM representation admission does not establish the retired
    // Regex allocator/slot policy for counted expansions. Keep that class on
    // Plan before trying the backend; its supported answers and limits remain.
    if linear.simple.is_none() && source.contains('{') {
        return Ok(false);
    }
    let source = if source == linear.form.as_ref() {
        linear.form.clone()
    } else {
        Arc::from(source)
    };
    readings.insert(source, linear);
    attempt.check().map_err(PreparationFailure::from)?;
    Ok(true)
}

pub(super) fn inline_multiple_token(token: &str) -> bool {
    token
        .split_once(['e', 'E'])
        .map_or(token, |(coefficient, _)| coefficient)
        .bytes()
        .filter(u8::is_ascii_digit)
        .count()
        <= 19
}

pub(super) fn multiple_token(token: &str) -> bool {
    let (coefficient, exponent) = token.split_once(['e', 'E']).unwrap_or((token, "0"));
    let digits = coefficient.bytes().filter(u8::is_ascii_digit).count();
    let Some(exponent) = exponent.parse::<i64>().ok().and_then(i64::checked_abs) else {
        return false;
    };
    digits <= 4096 && (exponent as u64).saturating_add(digits as u64) <= 20_000
}
