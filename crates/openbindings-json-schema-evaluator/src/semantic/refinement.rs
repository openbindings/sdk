//! Monotone coordinate proofs for recursive states. Facts become visible only
//! between attempts; an unfinished State never becomes a reusable summary.
use super::*;

#[derive(Default)]
pub(super) struct Facts {
    valid: Option<Formula>,
    properties: BTreeMap<Vec<u16>, Formula>,
    items: BTreeMap<usize, Formula>,
}
#[derive(Default)]
pub(super) struct Support {
    properties: BTreeMap<Vec<u16>, Formula>,
    items: BTreeMap<usize, Formula>,
    all_properties: bool,
    prefix_items: usize,
    all_items: bool,
}
impl Support {
    fn property(&self, name: &[u16]) -> Formula {
        if self.all_properties {
            TRUE
        } else {
            self.properties.get(name).copied().unwrap_or(FALSE)
        }
    }
    fn item(&self, index: usize) -> Formula {
        if self.all_items || index < self.prefix_items {
            TRUE
        } else {
            self.items.get(&index).copied().unwrap_or(FALSE)
        }
    }
}
fn conflict() -> NoVerdict {
    no_verdict(
        NoVerdictReason::EvaluatorFailure,
        "recursive-certificate-conflict",
        "recursive coordinate proofs are inconsistent",
    )
}

impl Run<'_> {
    /// Close only annotation propagation at this same instance. Child-instance
    /// producers cannot annotate their parent. A known cycle may propagate a
    /// known producer but cannot originate a property name or item position.
    fn discover_support(&mut self, root: &Visit) -> Result<Option<Support>, NoVerdict> {
        self.context_copy(&root.context)?;
        self.graph.charge(512)?;
        let mut pending = vec![(root.schema, root.context.clone())];
        let mut visited = BTreeSet::new();
        let mut support = Support::default();
        while let Some((node, incoming)) = pending.pop() {
            self.graph.tick()?;
            let context = self.enter(node, &incoming)?;
            self.context_copy(&context)?;
            self.graph.charge(256)?;
            let key = Visit {
                schema: node,
                instance: root.instance,
                context: context.clone(),
            };
            if visited.contains(&key) {
                continue;
            }
            // Leaves no longer publish completed memo entries. Their
            // preparation certificate still proves that they produce no
            // annotations, including under a foreign or custom profile.
            // This is structural absence, not an omitted-demand map.
            if self.plan.fields(node).is_leaf() {
                continue;
            }
            // A complete State already incorporates conditional selection and
            // failure gating. Its formulas bound possible annotation origins;
            // they never establish that a recursive caller produced one.
            if let Some(known) = self.memo.get(&key) {
                for (name, &formula) in &known.properties {
                    self.graph.work(name.len().saturating_add(1))?;
                    self.graph.charge(128 + name.len() * 2)?;
                    let previous = support.properties.get(name).copied().unwrap_or(FALSE);
                    support
                        .properties
                        .insert(name.clone(), self.graph.or(previous, formula)?);
                }
                for (&index, &formula) in &known.items {
                    self.graph.tick()?;
                    self.graph.charge(128)?;
                    let previous = support.items.get(&index).copied().unwrap_or(FALSE);
                    support
                        .items
                        .insert(index, self.graph.or(previous, formula)?);
                }
                continue;
            }
            visited.insert(key);
            let original = self.space().node(node);
            if original.profile != Some(Profile::standard(Dialect::Draft202012))
                || !self.space().eligible(original.resource)
                || self.space().query_inventory(original.resource).is_err()
            {
                return Ok(None);
            }
            let schema = original.value.clone();
            if schema.view().as_bool().is_some() {
                continue;
            }
            if schema.get("$id").is_some_and(|id| {
                id.as_str()
                    .is_none_or(|id| !openbindings_internal_schema::uri::valid(id))
            }) || schema.get("$dynamicRef").is_some()
                || schema.get("$recursiveRef").is_some()
            {
                return Ok(None);
            }
            if let Some(reference) = schema.get("$ref") {
                if reference.as_str().is_none() {
                    return Ok(None);
                }
                let query = self.query(node, "$ref")?;
                let Ok(target) = query.as_ref() else {
                    return Ok(None);
                };
                self.support_edge(&mut pending, target.node, &context)?;
            }
            for keyword in ["allOf", "anyOf", "oneOf"] {
                if let Some(children) = schema.get(keyword) {
                    let Some(children) = children.elements() else {
                        return Ok(None);
                    };
                    for child in children {
                        self.graph.tick()?;
                        self.support_edge(&mut pending, self.child(node, child)?, &context)?;
                    }
                }
            }
            if schema.get("if").is_some() {
                for keyword in ["if", "then", "else"] {
                    if let Some(child) = schema.get(keyword) {
                        self.support_edge(&mut pending, self.child(node, child)?, &context)?;
                    }
                }
            }
            if let Some(dependencies) = schema.get("dependentSchemas") {
                let Some(dependencies) = dependencies.members() else {
                    return Ok(None);
                };
                for dependency in dependencies {
                    self.graph.tick()?;
                    self.support_edge(&mut pending, self.child(node, dependency.value)?, &context)?;
                }
            }
            if self.plan.annotations.properties
                && let Some(properties) = schema.get("properties")
            {
                let Some(properties) = properties.members() else {
                    return Ok(None);
                };
                for property in properties {
                    let name = self.units(property.name)?;
                    self.graph.charge(128 + name.len() * 2)?;
                    support.properties.insert(name, TRUE);
                }
            }
            support.all_properties |= self.plan.annotations.properties
                && [
                    "patternProperties",
                    "additionalProperties",
                    "unevaluatedProperties",
                ]
                .iter()
                .any(|keyword| schema.get(keyword).is_some());
            if self.plan.annotations.items
                && let Some(prefix) = schema.get("prefixItems")
            {
                let Some(prefix) = prefix.elements() else {
                    return Ok(None);
                };
                support.prefix_items = support.prefix_items.max(prefix.len());
            }
            support.all_items |= self.plan.annotations.items
                && ["items", "contains", "unevaluatedItems"]
                    .iter()
                    .any(|keyword| schema.get(keyword).is_some());
            // not and propertyNames discard child evaluation annotations. The
            // instance-consuming schemas above originate only their selected
            // parent name/index; their own descendants are not propagation edges.
        }
        self.graph.tick()?;
        Ok(Some(support))
    }
    fn support_edge(
        &mut self,
        pending: &mut Vec<(usize, Context)>,
        node: usize,
        context: &Context,
    ) -> Result<(), NoVerdict> {
        self.context_copy(context)?;
        self.graph.charge(128)?;
        pending.push((node, context.clone()));
        Ok(())
    }
    fn annotation_support(&mut self, key: &Visit) -> Result<Arc<Option<Support>>, NoVerdict> {
        self.graph.tick()?;
        if let Some(known) = self.supports.get(key) {
            return Ok(known.clone());
        }
        let support = self.discover_support(key)?;
        self.context_copy(&key.context)?;
        self.graph.charge(512)?;
        let support = Arc::new(support);
        self.supports.insert(key.clone(), support.clone());
        Ok(support)
    }

    /// Replace only coordinates proved in an earlier round. Every unresolved
    /// bit remains a coherent provisional choice, gated by current validity.
    pub(super) fn recursive_cut(
        &mut self,
        key: &Visit,
        value: JsonRef<'_>,
    ) -> Result<State, NoVerdict> {
        let identity = self.values.intern(value, &mut self.graph)?;
        let valid = if let Some(valid) = self.facts.get(key).and_then(|facts| facts.valid) {
            valid
        } else {
            self.atom(
                Class::Inability,
                key.schema,
                "$cycle-cut".into(),
                Subject::Instance(identity),
                &key.context,
                None,
            )?
        };
        let mut state = State::truth(valid);
        if !((self.plan.annotations.properties && value.kind() == JsonKind::Object)
            || (self.plan.annotations.items && value.kind() == JsonKind::Array))
        {
            return Ok(state);
        }
        let support = self.annotation_support(key)?;
        if self.plan.annotations.properties
            && let Some(members) = value.members()
        {
            for member in members {
                let name = self.units(member.name)?;
                let bound = support
                    .as_ref()
                    .as_ref()
                    .map_or(TRUE, |support| support.property(&name));
                let known = self
                    .facts
                    .get(key)
                    .and_then(|facts| facts.properties.get(&name))
                    .copied();
                let selected = if let Some(known) = known {
                    if self.graph.and(known, bound)? != known {
                        return Err(conflict());
                    }
                    known
                } else if bound == FALSE {
                    continue;
                } else {
                    self.graph
                        .charge(name.len().saturating_mul(2).saturating_add(256))?;
                    let cut = self.atom(
                        Class::Inability,
                        key.schema,
                        "$cycle-cut/annotation".into(),
                        Subject::Member(identity, name.clone()),
                        &key.context,
                        None,
                    )?;
                    self.graph.and(cut, bound)?
                };
                self.graph
                    .charge(name.len().saturating_mul(2).saturating_add(128))?;
                state
                    .properties
                    .insert(name, self.graph.and(valid, selected)?);
            }
        }
        if self.plan.annotations.items
            && let Some(elements) = value.elements()
        {
            for (index, _) in elements.enumerate() {
                self.graph.tick()?;
                let bound = support
                    .as_ref()
                    .as_ref()
                    .map_or(TRUE, |support| support.item(index));
                let known = self
                    .facts
                    .get(key)
                    .and_then(|facts| facts.items.get(&index))
                    .copied();
                let selected = if let Some(known) = known {
                    if self.graph.and(known, bound)? != known {
                        return Err(conflict());
                    }
                    known
                } else if bound == FALSE {
                    continue;
                } else {
                    self.graph.charge(256)?;
                    let cut = self.atom(
                        Class::Inability,
                        key.schema,
                        format!("$cycle-cut/annotation/{index}"),
                        Subject::Instance(identity),
                        &key.context,
                        None,
                    )?;
                    self.graph.and(cut, bound)?
                };
                self.graph.charge(128)?;
                state.items.insert(index, self.graph.and(valid, selected)?);
            }
        }
        Ok(state)
    }

    pub(super) fn certify_coordinates(
        &mut self,
        key: &Visit,
        value: JsonRef<'_>,
        state: &State,
    ) -> Result<(), NoVerdict> {
        let mut next = Facts::default();
        let mut progress = false;
        if independent(state.valid, &mut self.graph, &self.cuts)? {
            match self.facts.get(key).and_then(|facts| facts.valid) {
                Some(known) if known != state.valid => return Err(conflict()),
                Some(_) => {}
                None => {
                    next.valid = Some(state.valid);
                    progress = true;
                }
            }
        }
        // Unobserved coordinates carry no fact. In particular, an omitted map
        // must never certify FALSE/absence for a later recursive attempt.
        if self.plan.annotations.properties
            && let Some(members) = value.members()
        {
            for member in members {
                let name = self.units(member.name)?;
                let formula = state.properties.get(&name).copied().unwrap_or(FALSE);
                if independent(formula, &mut self.graph, &self.cuts)? {
                    match self
                        .facts
                        .get(key)
                        .and_then(|facts| facts.properties.get(&name))
                    {
                        Some(&known) if known != formula => return Err(conflict()),
                        Some(_) => {}
                        None => {
                            self.graph.charge(128 + name.len() * 2)?;
                            next.properties.insert(name, formula);
                            progress = true;
                        }
                    }
                }
            }
        }
        if self.plan.annotations.items
            && let Some(elements) = value.elements()
        {
            for (index, _) in elements.enumerate() {
                self.graph.tick()?;
                let formula = state.items.get(&index).copied().unwrap_or(FALSE);
                if independent(formula, &mut self.graph, &self.cuts)? {
                    match self
                        .facts
                        .get(key)
                        .and_then(|facts| facts.items.get(&index))
                    {
                        Some(&known) if known != formula => return Err(conflict()),
                        Some(_) => {}
                        None => {
                            self.graph.charge(128)?;
                            next.items.insert(index, formula);
                            progress = true;
                        }
                    }
                }
            }
        }
        if progress {
            self.context_copy(&key.context)?;
            self.graph.charge(512)?;
            self.pending_facts.insert(key.clone(), next);
        }
        Ok(())
    }
    pub(super) fn commit_coordinates(&mut self) -> Result<bool, NoVerdict> {
        if self.pending_facts.is_empty() {
            return Ok(false);
        }
        for (key, next) in std::mem::take(&mut self.pending_facts) {
            self.graph
                .work(1 + next.properties.len() + next.items.len())?;
            self.graph.charge(256)?;
            let facts = self.facts.entry(key).or_default();
            if let Some(valid) = next.valid {
                facts.valid = Some(valid);
            }
            facts.properties.extend(next.properties);
            facts.items.extend(next.items);
        }
        Ok(true)
    }
}
