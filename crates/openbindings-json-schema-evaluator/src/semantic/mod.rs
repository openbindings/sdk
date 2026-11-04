//! Per-value dependence analysis over original schema locations.
//! The ordinary compiled engine remains the fast path. This layer represents
//! local effects and applicator annotations together when that path cannot
//! establish a value's result. It does not infer Undefined from failed bounds.
mod counts;
mod cycles;
mod decision;
mod formats;
mod meta;
mod operands;
mod patterns;
mod preparation;
mod primitive;
mod refinement;
mod regex_format;
mod values;
use self::decision::{Class, FALSE, Formula, Graph, TRUE};
use super::{Limits, no_verdict};
use openbindings::*;
use openbindings_internal_json::{backend, decimal::Decimal};
use openbindings_internal_schema::{
    OriginalEnvironment, QueryFailure, SchemaSpace,
    dialect::{Dialect, Profile},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

const RECURSIVE_SCOPE: &str = "#recursive-scope";

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ReferenceKind {
    Static,
    Dynamic,
    Recursive,
}
impl ReferenceKind {
    fn for_keyword(keyword: &str) -> Self {
        match keyword {
            "$ref" => Self::Static,
            "$dynamicRef" => Self::Dynamic,
            "$recursiveRef" => Self::Recursive,
            _ => unreachable!("reference keyword"),
        }
    }
}
type ReferenceQuery = Arc<Result<openbindings_internal_schema::Target, QueryFailure>>;
type PreparedQueries = HashMap<(usize, ReferenceKind), ReferenceQuery>;
type PreparedPatterns = HashMap<(usize, usize), Arc<crate::pattern::Reading>>;

/// Conservative demand over this Plan's complete activated original graph.
/// An omitted coordinate is unobserved, not a proof that it was unevaluated.
#[derive(Clone, Copy, Default)]
struct AnnotationDemand {
    properties: bool,
    items: bool,
}

#[derive(Clone, Copy)]
struct Interpreted<'a> {
    value: JsonRef<'a>,
    profile: Profile,
}
impl<'a> Interpreted<'a> {
    fn get(self, keyword: &str) -> Option<JsonRef<'a>> {
        self.profile.get(self.value, keyword)
    }
}

pub(super) struct Plan {
    environment: SchemaEnvironment,
    entry: usize,
    limits: Limits,
    preparation: PreparationLimits,
    custom: Vec<usize>,
    resources: Vec<usize>,
    formats: HashMap<usize, formats::Format>,
    queries: PreparedQueries,
    patterns: PreparedPatterns,
    operands: HashMap<usize, operands::Prepared>,
    annotations: AnnotationDemand,
    leaves: HashSet<usize>,
}
impl Plan {
    pub(super) fn new(
        environment: SchemaEnvironment,
        limits: Limits,
        preparation: PreparationLimits,
        control: &dyn openbindings_internal_schema::Control,
    ) -> Result<Self, PreparationFailure> {
        let entry = environment.entry();
        let plan = Self::at(environment, entry, limits, preparation, control)?;
        meta::validate(&plan, control)?;
        Ok(plan)
    }
    /// Evaluate an original schema position without constructing a replacement
    /// document or changing its resource base and dynamic scope.
    pub(super) fn at(
        environment: SchemaEnvironment,
        entry: usize,
        limits: Limits,
        preparation: PreparationLimits,
        control: &dyn openbindings_internal_schema::Control,
    ) -> Result<Self, PreparationFailure> {
        let checked = preparation::check_from(&environment, entry, &limits, control)?;
        Ok(Self {
            environment,
            entry,
            limits,
            preparation,
            custom: checked.custom,
            resources: checked.resources,
            formats: checked.formats,
            queries: checked.queries,
            patterns: checked.patterns,
            operands: checked.operands,
            annotations: checked.annotations,
            leaves: checked.leaves,
        })
    }
    pub(super) fn evaluate(
        &self,
        value: &JsonValue,
        _detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        self.evaluate_with_preparation(value, _detail, control, None)
    }
    pub(super) fn evaluate_with_preparation(
        &self,
        value: &JsonValue,
        _detail: Detail,
        control: &dyn WorkControl,
        attempt: Option<&dyn openbindings_internal_schema::Control>,
    ) -> ValueOutcome {
        self.evaluate_recording(value, control, attempt, &mut None)
    }
    fn evaluate_recording(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
        attempt: Option<&dyn openbindings_internal_schema::Control>,
        unavailable: &mut Option<String>,
    ) -> ValueOutcome {
        if let Err(cancelled) = control.check() {
            return ValueOutcome::NoVerdict {
                detail: cancelled.into(),
            };
        }
        if value.has_duplicate_names() {
            return ValueOutcome::NoVerdict {
                detail: no_verdict(
                    NoVerdictReason::UnsupportedCapability,
                    "duplicate-instance-members",
                    "a value with repeated JSON member names has no unambiguous interpretation",
                ),
            };
        }
        let depth = (self.limits.stack_bytes / 4096).max(1);
        let mut run = Run {
            plan: self,
            graph: Graph::with_preparation(
                self.limits.program_bytes,
                self.limits.evaluation_steps,
                depth,
                control,
                attempt,
            ),
            atoms: Vec::new(),
            values: values::Values::default(),
            effects: HashMap::new(),
            active: Vec::new(),
            opaque_cycles: HashSet::new(),
            pending_cycles: HashSet::new(),
            cuts: HashSet::new(),
            provisional: HashSet::new(),
            retry: false,
            memo: HashMap::new(),
            facts: HashMap::new(),
            pending_facts: HashMap::new(),
            supports: HashMap::new(),
            support_memo_count: 0,
            patterns: HashMap::new(),
            depth: 0,
            maximum_depth: depth,
        };
        loop {
            let result = run.schema(self.entry, value.view(), &Vec::new());
            if run.retry {
                run.retry = false;
                match run.finish_attempt() {
                    Ok(true) => continue,
                    Ok(false) => {
                        return run.finish_root(
                            value.view(),
                            ValueOutcome::NoVerdict {
                                detail: cycle_inability(),
                            },
                            unavailable,
                        );
                    }
                    Err(detail) => return ValueOutcome::NoVerdict { detail },
                }
            }
            let result = result.and_then(|state| {
                if run.pending_cycles.is_empty()
                    || independent(state.valid, &mut run.graph, &run.cuts)?
                {
                    return run.conclude(state.valid, unavailable).map(Some);
                }
                // An enclosing applicator may eliminate a provisional cut. Only
                // make unresolved components opaque after completing that proof
                // attempt. Certified summaries remain valid across retries.
                if run.finish_attempt()? {
                    Ok(None)
                } else {
                    Err(cycle_inability())
                }
            });
            match result {
                Ok(Some(outcome)) => return run.finish_root(value.view(), outcome, unavailable),
                Ok(None) => {}
                Err(detail) => {
                    return run.finish_root(
                        value.view(),
                        ValueOutcome::NoVerdict { detail },
                        unavailable,
                    );
                }
            }
        }
    }
}
fn preparation_control_inability(failure: openbindings_internal_schema::Failure) -> NoVerdict {
    preparation_inability(failure.into())
}
fn preparation_inability(failure: PreparationFailure) -> NoVerdict {
    let (reason, code) = match failure.kind {
        PreparationFailureKind::Cancelled => {
            (NoVerdictReason::Cancelled, "caller-cancelled".into())
        }
        PreparationFailureKind::Limit { resource } => (NoVerdictReason::LimitExceeded, resource),
        PreparationFailureKind::UnsupportedCapability { capability } => {
            (NoVerdictReason::UnsupportedCapability, capability)
        }
        _ => (
            NoVerdictReason::UnsupportedCapability,
            "semantic-predicate-preparation".into(),
        ),
    };
    no_verdict(
        reason,
        &code,
        "semantic analysis could not prepare a primitive predicate",
    )
}
fn cycle_inability() -> NoVerdict {
    no_verdict(
        NoVerdictReason::UnsupportedCapability,
        "semantic-cycle-summary",
        "a recursive summary remains provisional",
    )
}
// Effective outermost bindings, not an ever-growing resource-entry history.
type Context = Vec<(String, Option<usize>)>;
fn reserve_context(graph: &mut Graph<'_>, context: &Context) -> Result<(), NoVerdict> {
    let mut bytes = 256usize;
    for (name, _) in context {
        graph.work(name.len().saturating_mul(16).saturating_add(1))?;
        bytes = bytes.saturating_add(name.len().saturating_mul(2).saturating_add(256));
    }
    graph.charge(bytes)
}
#[derive(Clone, PartialEq, Eq, Hash)]
struct Visit {
    schema: usize,
    instance: usize,
    context: Context,
}
#[derive(Clone, PartialEq, Eq, Hash)]
enum Subject {
    Instance(usize),
    Text(Vec<u16>),
    Member(usize, Vec<u16>),
}
#[derive(Clone, PartialEq, Eq, Hash)]
struct Effect {
    node: usize,
    predicate: String,
    subject: Subject,
    context: Context,
}
struct Atom {
    class: Class,
    node: usize,
    keyword: String,
    uri: Option<String>,
    cause: Option<NoVerdict>,
}
#[derive(Clone)]
struct State {
    valid: Formula,
    properties: BTreeMap<Vec<u16>, Formula>,
    items: BTreeMap<usize, Formula>,
}
impl State {
    fn copied(&self, graph: &mut Graph<'_>) -> Result<Self, NoVerdict> {
        graph.work(
            self.items
                .len()
                .saturating_add(self.properties.len())
                .saturating_add(1),
        )?;
        let mut bytes = 256usize.saturating_add(self.items.len().saturating_mul(128));
        for name in self.properties.keys() {
            graph.work(name.len().saturating_add(1))?;
            bytes = bytes.saturating_add(name.len().saturating_mul(2).saturating_add(256));
        }
        graph.charge(bytes)?;
        Ok(self.clone())
    }
    fn truth(valid: Formula) -> Self {
        Self {
            valid,
            properties: BTreeMap::new(),
            items: BTreeMap::new(),
        }
    }
    fn independent(&self, graph: &mut Graph<'_>, cuts: &HashSet<u32>) -> Result<bool, NoVerdict> {
        for formula in std::iter::once(&self.valid)
            .chain(self.properties.values())
            .chain(self.items.values())
        {
            if !independent(*formula, graph, cuts)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn independent(
    formula: Formula,
    graph: &mut Graph<'_>,
    cuts: &HashSet<u32>,
) -> Result<bool, NoVerdict> {
    if cuts.is_empty() || formula < 2 {
        return Ok(true);
    }
    Ok(graph.support(formula)?.iter().all(|id| !cuts.contains(id)))
}

struct Run<'a> {
    plan: &'a Plan,
    graph: Graph<'a>,
    atoms: Vec<Atom>,
    values: values::Values,
    effects: HashMap<Effect, Formula>,
    active: Vec<Visit>,
    opaque_cycles: HashSet<Visit>,
    pending_cycles: HashSet<Visit>,
    cuts: HashSet<u32>,
    provisional: HashSet<Visit>,
    retry: bool,
    memo: HashMap<Visit, State>,
    facts: HashMap<Visit, refinement::Facts>,
    pending_facts: HashMap<Visit, refinement::Facts>,
    supports: HashMap<Visit, Arc<Option<refinement::Support>>>,
    support_memo_count: usize,
    patterns: HashMap<(usize, usize), patterns::Pattern>,
    depth: usize,
    maximum_depth: usize,
}
impl Run<'_> {
    fn units(&mut self, value: JsonRef<'_>) -> Result<Vec<u16>, NoVerdict> {
        let bound = value.text().len();
        self.graph
            .work(bound.saturating_mul(16).saturating_add(1))?;
        self.graph
            .charge(bound.saturating_mul(2).saturating_add(32))?;
        let mut units = Vec::with_capacity(bound);
        units.extend(value.as_string().expect("string").utf16_units());
        Ok(units)
    }
    fn copy_uri(&mut self, uri: Option<&str>) -> Result<Option<String>, NoVerdict> {
        let Some(uri) = uri else {
            return Ok(None);
        };
        self.graph.work(uri.len().saturating_add(1))?;
        self.graph.charge(uri.len().saturating_add(32))?;
        Ok(Some(uri.into()))
    }
    fn query(&mut self, node: usize, keyword: &str) -> Result<ReferenceQuery, NoVerdict> {
        self.graph.tick()?;
        self.plan
            .queries
            .get(&(node, ReferenceKind::for_keyword(keyword)))
            .cloned()
            .ok_or_else(|| {
                no_verdict(
                    NoVerdictReason::EvaluatorFailure,
                    "unprepared-reference-query",
                    "the evaluator reached a reference outside its prepared original-node graph",
                )
            })
    }
    fn context_copy(&mut self, context: &Context) -> Result<(), NoVerdict> {
        reserve_context(&mut self.graph, context)
    }
    fn finish_attempt(&mut self) -> Result<bool, NoVerdict> {
        // Only an earlier completed round supplies facts to the next one. Do
        // not discard a still-refinable component into the opaque fallback.
        let new_coordinates = self.commit_coordinates()?;
        let new_complete_states = self.memo.len() > self.support_memo_count;
        if new_complete_states {
            self.support_memo_count = self.memo.len();
            self.supports.clear();
        }
        if new_coordinates || new_complete_states {
            self.pending_cycles.clear();
            self.provisional.clear();
            return Ok(true);
        }
        let mut progress = false;
        for visit in self.pending_cycles.drain() {
            if !self.memo.contains_key(&visit) {
                progress |= self.opaque_cycles.insert(visit);
            }
        }
        self.provisional.clear();
        Ok(progress)
    }
    fn space(&self) -> &SchemaSpace {
        self.plan.environment.space()
    }
    fn child(&self, holder: usize, child: JsonRef<'_>) -> Result<usize, NoVerdict> {
        let source = self.space().node(holder).source;
        self.space()
            .source(source)
            .positions
            .get(&backend::ref_id(child))
            .copied()
            .ok_or_else(|| {
                no_verdict(
                    NoVerdictReason::UnsupportedCapability,
                    "semantic-schema-position",
                    "the original inventory does not establish this subschema position",
                )
            })
    }
    fn enter(&mut self, node: usize, context: &Context) -> Result<Context, NoVerdict> {
        self.context_copy(context)?;
        let space = self.plan.environment.space();
        let resource = space.resource(space.node(node).resource);
        let mut out: BTreeMap<_, _> = context.iter().cloned().collect();
        for (name, anchors) in &resource.anchors {
            self.graph
                .work(name.len().saturating_mul(16).saturating_add(anchors.len()))?;
            if anchors.iter().any(|(_, dynamic)| *dynamic) {
                if out.contains_key(name) {
                    continue;
                }
                self.graph
                    .charge(name.len().saturating_mul(2).saturating_add(256))?;
                out.entry(name.clone()).or_insert_with(|| {
                    if anchors.len() == 1 {
                        Some(anchors[0].0)
                    } else {
                        None
                    }
                });
            }
        }
        let original = space.node(node);
        if original.profile == Some(Profile::standard(Dialect::Draft201909))
            && original
                .value
                .get("$recursiveAnchor")
                .and_then(|v| v.as_bool())
                == Some(true)
        {
            self.graph.charge(256)?;
            out.entry(RECURSIVE_SCOPE.into()).or_insert(resource.root);
        }
        self.graph.charge(out.len().saturating_mul(64))?;
        Ok(out.into_iter().collect())
    }
    fn atom(
        &mut self,
        class: Class,
        node: usize,
        predicate: String,
        subject: Subject,
        context: &Context,
        uri: Option<String>,
    ) -> Result<Formula, NoVerdict> {
        // An external schema cannot name the OBI document resource (§7.2).
        // Keep this original predicate private only when the inventory proves
        // that boundary; every identified/supplied case stays conservative.
        let class = if class == Class::Undefined && node != usize::MAX {
            let original = self.space().node(node);
            let resource = self.space().resource(original.resource);
            if resource.anonymous
                && resource.origin == openbindings_internal_schema::ResourceOrigin::Contained
                && self.space().source(original.source).uri.is_none()
            {
                Class::PrivateUndefined
            } else {
                class
            }
        } else {
            class
        };
        self.context_copy(context)?;
        if let Subject::Text(units) | Subject::Member(_, units) = &subject {
            self.graph
                .work(units.len().saturating_mul(4).saturating_add(1))?;
        }
        self.graph.work(predicate.len().saturating_add(1))?;
        self.graph.charge(predicate.len().saturating_add(256))?;
        let key = Effect {
            node,
            predicate: predicate.clone(),
            subject,
            context: context.clone(),
        };
        if let Some(&known) = self.effects.get(&key) {
            return Ok(known);
        }
        let text_bytes = predicate.len()
            + context
                .iter()
                .map(|(name, _)| name.len() + 32)
                .sum::<usize>()
            + match &key.subject {
                Subject::Text(units) | Subject::Member(_, units) => units.len() * 2,
                Subject::Instance(_) => 0,
            };
        self.graph.charge(256usize.saturating_add(text_bytes))?;
        let (index, formula) = self.graph.variable(class)?;
        assert_eq!(index as usize, self.atoms.len());
        self.atoms.push(Atom {
            class,
            node,
            keyword: predicate,
            uri,
            cause: None,
        });
        self.effects.insert(key, formula);
        Ok(formula)
    }
    fn cause(&mut self, formula: Formula, detail: NoVerdict) -> Result<(), NoVerdict> {
        let support = self.graph.support(formula)?;
        let index = *support.first().expect("an effect is a variable");
        let atom = &mut self.atoms[index as usize];
        if atom.cause.is_none() {
            self.graph
                .charge(detail.code.len() + detail.message.len() + 128)?;
            atom.cause = Some(detail);
        }
        Ok(())
    }
    fn effect(
        &mut self,
        class: Class,
        node: usize,
        keyword: &str,
        value: JsonRef<'_>,
        context: &Context,
        uri: Option<String>,
    ) -> Result<State, NoVerdict> {
        let identity = self.values.intern(value, &mut self.graph)?;
        let valid = self.atom(
            class,
            node,
            keyword.into(),
            Subject::Instance(identity),
            context,
            uri,
        )?;
        Ok(State::truth(valid))
    }
    fn unknown_schema(
        &mut self,
        class: Class,
        node: usize,
        keyword: &str,
        value: JsonRef<'_>,
        context: &Context,
        uri: Option<String>,
    ) -> Result<State, NoVerdict> {
        let identity = self.values.intern(value, &mut self.graph)?;
        let copied_uri = self.copy_uri(uri.as_deref())?;
        let mut state = self.effect(class, node, keyword, value, context, copied_uri)?;
        let valid = state.valid;
        // Unknown schema contents may produce annotations. This is a conservative
        // enlargement, so its variables cannot witness an Undefined conclusion.
        // A reference's possible annotations follow its same validity choice.
        // Finite property/index selections are coherent across repeated calls.
        if self.plan.annotations.properties
            && let Some(members) = value.members()
        {
            for member in members {
                let name = self.units(member.name)?;
                self.graph.charge(
                    name.len()
                        .saturating_mul(2)
                        .saturating_add(keyword.len())
                        .saturating_add(256),
                )?;
                let copied_uri = self.copy_uri(uri.as_deref())?;
                let selected = self.atom(
                    class,
                    node,
                    format!("{keyword}/annotation"),
                    Subject::Member(identity, name.clone()),
                    context,
                    copied_uri,
                )?;
                let selected = self.graph.and(valid, selected)?;
                self.graph.charge(96 + name.len() * 2)?;
                state.properties.insert(name, selected);
            }
        }
        if self.plan.annotations.items
            && let Some(elements) = value.elements()
        {
            for (index, _) in elements.enumerate() {
                self.graph.charge(keyword.len().saturating_add(256))?;
                let copied_uri = self.copy_uri(uri.as_deref())?;
                let selected = self.atom(
                    class,
                    node,
                    format!("{keyword}/annotation/{index}"),
                    Subject::Instance(identity),
                    context,
                    copied_uri,
                )?;
                self.graph.charge(96)?;
                state.items.insert(index, self.graph.and(valid, selected)?);
            }
        }
        Ok(state)
    }
    fn annotations(
        &mut self,
        target: &mut State,
        source: &State,
        guard: Formula,
    ) -> Result<(), NoVerdict> {
        for (name, &formula) in &source.properties {
            let guarded = self.graph.and(formula, guard)?;
            let old = target.properties.get(name).copied().unwrap_or(FALSE);
            let combined = self.graph.or(old, guarded)?;
            self.graph.charge(96 + name.len() * 2)?;
            target.properties.insert(name.clone(), combined);
        }
        for (&index, &formula) in &source.items {
            let guarded = self.graph.and(formula, guard)?;
            let old = target.items.get(&index).copied().unwrap_or(FALSE);
            target.items.insert(index, self.graph.or(old, guarded)?);
            self.graph.charge(96)?;
        }
        Ok(())
    }
    fn conjunction(&mut self, target: &mut State, other: &State) -> Result<(), NoVerdict> {
        target.valid = self.graph.and(target.valid, other.valid)?;
        self.annotations(target, other, TRUE)
    }
    fn alias(
        &mut self,
        mut node: usize,
        incoming: &Context,
    ) -> Result<(usize, Context, bool), NoVerdict> {
        self.context_copy(incoming)?;
        let mut context = incoming.clone();
        let mut path = Vec::<(usize, Context, Option<String>)>::new();
        loop {
            self.graph.tick()?;
            context = self.enter(node, &context)?;
            if let Some(start) = path
                .iter()
                .position(|(prior, scope, _)| *prior == node && *scope == context)
            {
                let members = &path[start..];
                let canonical = members
                    .iter()
                    .map(|(node, _, _)| *node)
                    .min()
                    .expect("nonempty cycle");
                self.context_copy(&context)?;
                let relevant: Context = context
                    .iter()
                    .filter(|(name, _)| {
                        members
                            .iter()
                            .any(|(_, _, used)| used.as_ref() == Some(name))
                    })
                    .cloned()
                    .collect();
                return Ok((canonical, relevant, true));
            }
            let original = self.space().node(node);
            let schema = original.value.clone();
            if !self.space().eligible(original.resource)
                || original.profile.is_none()
                || self.space().query_inventory(original.resource).is_err()
                || schema.get("$id").is_some_and(|id| {
                    id.as_str()
                        .is_none_or(|id| !openbindings_internal_schema::uri::valid(id))
                })
            {
                return Ok((node, context, false));
            }
            let profile = original.profile.expect("known alias profile");
            let mut alias = None;
            let Some(fields) = schema.view().members() else {
                return Ok((node, context, false));
            };
            for field in fields {
                if field.name.as_str().is_some_and(|key| {
                    !profile.active(key)
                        || (profile.reference_object(schema.view()) && key != "$ref")
                }) {
                    continue;
                }
                match field.name.as_str() {
                    Some(
                        keyword @ ("$ref" | "$dynamicRef" | "$recursiveRef" | "allOf" | "anyOf"
                        | "oneOf"),
                    ) if alias.is_none() => alias = Some((keyword, field.value)),
                    Some(
                        "$schema" | "$id" | "$anchor" | "$dynamicAnchor" | "$recursiveAnchor"
                        | "$defs" | "definitions" | "$comment" | "title" | "description"
                        | "default" | "examples" | "deprecated" | "readOnly" | "writeOnly"
                        | "contentEncoding" | "contentMediaType" | "contentSchema",
                    ) => {}
                    Some("format") if !profile.asserts_format() => {}
                    _ => return Ok((node, context, false)),
                }
            }
            let Some((keyword, operand)) = alias else {
                return Ok((node, context, false));
            };
            let (selected, dynamic) = if matches!(keyword, "allOf" | "anyOf" | "oneOf") {
                let Some(mut children) = operand.elements() else {
                    return Ok((node, context, false));
                };
                let Some(child) = children.next() else {
                    return Ok((node, context, false));
                };
                if children.next().is_some() {
                    return Ok((node, context, false));
                }
                (self.child(node, child)?, None)
            } else {
                let Some(reference) = operand.as_str() else {
                    return Ok((node, context, false));
                };
                if keyword == "$recursiveRef" && reference != "#" {
                    return Ok((node, context, false));
                }
                let query = self.query(node, keyword)?;
                let Ok(target) = query.as_ref() else {
                    return Ok((node, context, false));
                };
                if let Some(name) = &target.dynamic_name {
                    self.graph.work(name.len().saturating_add(1))?;
                    self.graph.charge(name.len().saturating_add(64))?;
                }
                let dynamic = if keyword == "$recursiveRef" && self.recursive_target(target.node) {
                    Some(RECURSIVE_SCOPE.into())
                } else {
                    (keyword == "$dynamicRef")
                        .then(|| target.dynamic_name.clone())
                        .flatten()
                };
                let selected = if let Some(name) = &dynamic {
                    match context.iter().find(|(binding, _)| binding == name) {
                        Some((_, Some(selected))) => *selected,
                        Some((_, None)) => return Ok((node, context, false)),
                        None => target.node,
                    }
                } else {
                    target.node
                };
                (selected, dynamic)
            };
            self.graph
                .charge(256 + context.iter().map(|(s, _)| s.len() + 32).sum::<usize>())?;
            path.push((node, context, dynamic));
            context = path.last().expect("pushed alias").1.clone();
            node = selected;
        }
    }
    fn schema(
        &mut self,
        node: usize,
        value: JsonRef<'_>,
        context: &Context,
    ) -> Result<State, NoVerdict> {
        self.graph.tick()?;
        if self.depth >= self.maximum_depth {
            return Err(decision::limit("semantic-analysis-depth-limit"));
        }
        // A preparation-certified leaf cannot recurse or propagate child
        // annotations. Keep scope and predicate semantics without retaining a
        // Visit/State for every ordinary array item or object member.
        if self.plan.leaves.contains(&node) {
            let context = self.enter(node, context)?;
            return self.body(node, value, &context);
        }
        let (node, context, pure_cycle) = self.alias(node, context)?;
        if pure_cycle {
            return self.effect(
                Class::Undefined,
                node,
                "$alias-cycle",
                value,
                &context,
                None,
            );
        }
        self.context_copy(&context)?;
        let key = Visit {
            schema: node,
            instance: backend::ref_id(value),
            context: context.clone(),
        };
        if let Some(known) = self.memo.get(&key) {
            return known.copied(&mut self.graph);
        }
        if self.opaque_cycles.contains(&key) {
            return self.unknown_schema(
                Class::Inability,
                node,
                "$cycle-summary",
                value,
                &context,
                None,
            );
        }
        if let Some(start) = self.active.iter().position(|visit| visit == &key) {
            for visit in &self.active[start..] {
                reserve_context(&mut self.graph, &visit.context)?;
                self.graph.charge(256)?;
                self.pending_cycles.insert(visit.clone());
            }
            // A cut is provisional, never the meaning of a recursive body. Its
            // namespace differs from the persistent opaque fallback summary.
            // Parent frames may eliminate every cut before certification.
            let first = self.atoms.len();
            let result = self.recursive_cut(&key, value)?;
            for id in first..self.atoms.len() {
                self.graph.charge(32)?;
                self.cuts.insert(id as u32);
            }
            return Ok(result);
        }
        if self.provisional.contains(&key) {
            // A second unfolding may use a different cut context. Reusing its
            // partial State is unsound, while enumerating paths can explode.
            // Leave this speculative attempt before repeating provisional work.
            self.retry = true;
            return Err(cycle_inability());
        }
        self.context_copy(&context)?;
        self.graph
            .charge(256 + context.iter().map(|(s, _)| s.len() + 32).sum::<usize>())?;
        self.active.push(key.clone());
        self.depth += 1;
        let result = self.body(node, value, &context);
        self.depth -= 1;
        self.active.pop();
        let mut result = result?;
        // Failed schemas produce no usable evaluation annotations.
        for annotation in result
            .properties
            .values_mut()
            .chain(result.items.values_mut())
        {
            *annotation = self.graph.and(result.valid, *annotation)?;
        }
        let complete = result.independent(&mut self.graph, &self.cuts)?;
        if !complete || self.facts.contains_key(&key) {
            self.certify_coordinates(&key, value, &result)?;
        }
        if complete {
            let copied = result.copied(&mut self.graph)?;
            self.graph.charge(256)?;
            self.memo.insert(key, copied);
        } else {
            self.graph.charge(256)?;
            self.provisional.insert(key);
        }
        Ok(result)
    }
    fn body(
        &mut self,
        node: usize,
        value: JsonRef<'_>,
        context: &Context,
    ) -> Result<State, NoVerdict> {
        let schema = self.space().node(node).value.clone();
        if let Some(boolean) = schema.view().as_bool() {
            return Ok(State::truth(if boolean { TRUE } else { FALSE }));
        }
        if !self.space().eligible(self.space().node(node).resource) {
            return self.unknown_schema(Class::Inability, node, "$identity", value, context, None);
        }
        if self
            .space()
            .query_inventory(self.space().node(node).resource)
            .is_err()
        {
            return self.unknown_schema(Class::Inability, node, "$dialect", value, context, None);
        }
        let Some(profile) = self.space().node(node).profile else {
            return self.unknown_schema(Class::Inability, node, "$dialect", value, context, None);
        };
        let schema = Interpreted {
            value: schema.view(),
            profile,
        };
        if profile.reference_object(schema.value) {
            return self.reference(
                node,
                "$ref",
                schema.get("$ref").expect("reference object"),
                value,
                context,
            );
        }
        let mut state = State::truth(TRUE);
        // Identifier declarations establish resource/base context, not an
        // assertion about this instance. A malformed identifier matters when
        // a reference actually depends on that context; reference() retains
        // the original query failure instead of inventing a validity atom.
        match self.primitives(node, &schema, value) {
            Ok(true) => {}
            Ok(false) => state.valid = FALSE,
            Err(detail)
                if matches!(
                    detail.reason,
                    NoVerdictReason::LimitExceeded | NoVerdictReason::Cancelled
                ) =>
            {
                return Err(detail);
            }
            Err(detail) => {
                let unknown =
                    self.effect(Class::Inability, node, "$predicate", value, context, None)?;
                self.cause(unknown.valid, detail)?;
                self.conjunction(&mut state, &unknown)?;
            }
        }
        if let Some(format) = self.plan.formats.get(&node) {
            match format.evaluate(value, &mut self.graph) {
                Ok(true) => {}
                Ok(false) => state.valid = FALSE,
                Err(detail)
                    if matches!(
                        detail.reason,
                        NoVerdictReason::LimitExceeded | NoVerdictReason::Cancelled
                    ) =>
                {
                    return Err(detail);
                }
                Err(detail) => {
                    let unknown =
                        self.effect(Class::Inability, node, "format", value, context, None)?;
                    self.cause(unknown.valid, detail)?;
                    self.conjunction(&mut state, &unknown)?;
                }
            }
        }
        if let Some(pattern) = schema.get("pattern")
            && value.kind() == JsonKind::String
        {
            let matched = self.pattern(node, "pattern", pattern, value, context)?;
            state.valid = self.graph.and(state.valid, matched)?;
        }
        if self.plan.leaves.contains(&node) {
            return Ok(state);
        }
        for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            if let Some(reference) = schema.get(keyword) {
                let other = self.reference(node, keyword, reference, value, context)?;
                self.conjunction(&mut state, &other)?;
            }
        }
        for keyword in ["allOf", "anyOf", "oneOf"] {
            if let Some(branches) = schema.get(keyword).and_then(|v| v.elements()) {
                let mut combined = State::truth(if keyword == "allOf" { TRUE } else { FALSE });
                let mut one = FALSE;
                let mut zero = TRUE;
                for branch in branches {
                    let child = self.child(node, branch)?;
                    let result = self.schema(child, value, context)?;
                    combined.valid = if keyword == "allOf" {
                        self.graph.and(combined.valid, result.valid)?
                    } else {
                        self.graph.or(combined.valid, result.valid)?
                    };
                    if keyword == "oneOf" {
                        let chosen = self.graph.and(zero, result.valid)?;
                        let no = self.graph.not(result.valid)?;
                        let prior = self.graph.and(one, no)?;
                        one = self.graph.or(chosen, prior)?;
                        zero = self.graph.and(zero, no)?;
                    }
                    self.annotations(&mut combined, &result, TRUE)?;
                }
                if keyword == "oneOf" {
                    combined.valid = one;
                }
                self.conjunction(&mut state, &combined)?;
            }
        }
        if let Some(child) = schema.get("not") {
            let result = self.schema(self.child(node, child)?, value, context)?;
            let not = self.graph.not(result.valid)?;
            state.valid = self.graph.and(state.valid, not)?;
        }
        if let Some(child) = schema.get("if") {
            let condition = self.schema(self.child(node, child)?, value, context)?;
            let yes = if let Some(child) = schema.get("then") {
                self.schema(self.child(node, child)?, value, context)?
            } else {
                State::truth(TRUE)
            };
            let no = if let Some(child) = schema.get("else") {
                self.schema(self.child(node, child)?, value, context)?
            } else {
                State::truth(TRUE)
            };
            let selected = self.graph.choose(condition.valid, yes.valid, no.valid)?;
            state.valid = self.graph.and(state.valid, selected)?;
            self.annotations(&mut state, &condition, TRUE)?;
            self.annotations(&mut state, &yes, condition.valid)?;
            let otherwise = self.graph.not(condition.valid)?;
            self.annotations(&mut state, &no, otherwise)?;
        }
        if value.kind() == JsonKind::Object {
            self.object(node, &schema, value, context, &mut state)?;
        }
        if value.kind() == JsonKind::Array {
            self.array(node, &schema, value, context, &mut state)?;
        }
        Ok(state)
    }
    fn recursive_target(&self, node: usize) -> bool {
        let target = self.space().node(node);
        target.profile == Some(Profile::standard(Dialect::Draft201909))
            && target
                .value
                .get("$recursiveAnchor")
                .and_then(|v| v.as_bool())
                == Some(true)
    }
    fn reference(
        &mut self,
        node: usize,
        keyword: &str,
        reference: JsonRef<'_>,
        value: JsonRef<'_>,
        context: &Context,
    ) -> Result<State, NoVerdict> {
        let Some(reference) = reference.as_str() else {
            // A JSON string containing an unpaired unit cannot be a URI
            // reference. Keep that local undefined fact visible to masking
            // applicators, just as for a malformed scalar URI reference.
            let class = if reference.kind() == JsonKind::String {
                Class::Undefined
            } else {
                Class::Inability
            };
            return self.effect(class, node, keyword, value, context, None);
        };
        if keyword == "$recursiveRef" && reference != "#" {
            return self.effect(Class::Inability, node, keyword, value, context, None);
        }
        let query = self.query(node, keyword)?;
        match query.as_ref() {
            Ok(target) => {
                let recursive = keyword == "$recursiveRef" && self.recursive_target(target.node);
                let selected = if keyword == "$dynamicRef" || recursive {
                    if let Some(name) = if recursive {
                        Some(RECURSIVE_SCOPE)
                    } else {
                        target.dynamic_name.as_deref()
                    } {
                        match context.iter().find(|(binding, _)| binding == name) {
                            Some((_, Some(target))) => *target,
                            Some((_, None)) => {
                                return self.effect(
                                    Class::Undefined,
                                    node,
                                    keyword,
                                    value,
                                    context,
                                    None,
                                );
                            }
                            None => target.node,
                        }
                    } else {
                        target.node
                    }
                } else {
                    target.node
                };
                self.schema(selected, value, context)
            }
            Err(QueryFailure::ResourceUnavailable(uri)) => {
                // The resource identity and original fragment identify one
                // predicate across all referrers in this environment.
                let fragment = reference.split_once('#').map_or("", |(_, f)| f);
                self.graph
                    .work(uri.len().saturating_add(fragment.len()).saturating_add(1))?;
                self.graph.charge(
                    uri.len()
                        .saturating_mul(4)
                        .saturating_add(fragment.len())
                        .saturating_add(512),
                )?;
                let predicate = format!("$external:{uri}#{fragment}");
                self.unknown_schema(
                    Class::External,
                    usize::MAX,
                    &predicate,
                    value,
                    context,
                    Some(uri.clone()),
                )
            }
            Err(
                QueryFailure::IdentityConflict
                | QueryFailure::UnsupportedDialect(_)
                | QueryFailure::UnsupportedCapability(_)
                | QueryFailure::InvalidExternalSchema,
            ) => self.unknown_schema(Class::Inability, node, keyword, value, context, None),
            Err(_) => self.effect(Class::Undefined, node, keyword, value, &Vec::new(), None),
        }
    }
    fn pattern_atom(
        &mut self,
        class: Class,
        node: usize,
        keyword: &str,
        value: JsonRef<'_>,
    ) -> Result<Formula, NoVerdict> {
        // Linear matching borrows UTF-8 directly. Only an unknown match needs an
        // exact text key; reserve its allowance before decoding or retaining it.
        let units = self.units(value)?;
        self.atom(
            class,
            node,
            keyword.into(),
            Subject::Text(units),
            &Vec::new(),
            None,
        )
    }
    fn pattern(
        &mut self,
        node: usize,
        keyword: &str,
        pattern: JsonRef<'_>,
        value: JsonRef<'_>,
        _context: &Context,
    ) -> Result<Formula, NoVerdict> {
        if pattern.as_str().is_none() {
            return self.pattern_atom(Class::Inability, node, keyword, value);
        }
        let key = backend::occurrence(pattern);
        if !self.patterns.contains_key(&key) {
            let reading = self.plan.patterns.get(&key).ok_or_else(|| {
                no_verdict(
                    NoVerdictReason::EvaluatorFailure,
                    "unprepared-pattern-reading",
                    "the original pattern operand has no prepared interpretation",
                )
            })?;
            self.graph.charge(512)?;
            let compiled = patterns::Pattern::prepare(reading, &self.plan.limits, &mut self.graph)?;
            self.patterns.insert(key, compiled);
        }
        match self
            .patterns
            .get_mut(&key)
            .expect("prepared pattern")
            .evaluate(value.as_str(), &mut self.graph)?
        {
            patterns::ResultKind::Matched(matched) => Ok(if matched { TRUE } else { FALSE }),
            patterns::ResultKind::Undefined => {
                self.pattern_atom(Class::Undefined, node, keyword, value)
            }
            patterns::ResultKind::Unsupported(why) => {
                let formula = self.pattern_atom(Class::Inability, node, keyword, value)?;
                self.cause(formula, super::unsupported_pattern(why, "pattern"))?;
                Ok(formula)
            }
            patterns::ResultKind::Unpaired => {
                let formula = self.pattern_atom(Class::Inability, node, keyword, value)?;
                self.cause(
                    formula,
                    no_verdict(
                        NoVerdictReason::UnsupportedCapability,
                        "lone-surrogate-instance",
                        "the pattern matcher cannot interpret an unpaired UTF-16 code unit",
                    ),
                )?;
                Ok(formula)
            }
        }
    }
    fn named<'v>(
        &mut self,
        object: JsonRef<'v>,
        name: JsonRef<'_>,
    ) -> Result<Option<JsonRef<'v>>, NoVerdict> {
        if let Some(name) = name.as_str() {
            return Ok(object.get(name));
        }
        let exact = name.as_string().expect("member name");
        for member in object.members().into_iter().flatten() {
            self.graph.tick()?;
            if member.name.as_string().expect("member name") == exact {
                return Ok(Some(member.value));
            }
        }
        Ok(None)
    }
    fn object(
        &mut self,
        node: usize,
        schema: &Interpreted<'_>,
        value: JsonRef<'_>,
        context: &Context,
        state: &mut State,
    ) -> Result<(), NoVerdict> {
        if let Some(dependencies) = schema
            .get("dependentSchemas")
            .or_else(|| schema.get("dependencies"))
            .and_then(|v| v.members())
        {
            for dependency in dependencies {
                if dependency.value.kind() == JsonKind::Array {
                    continue;
                }
                if self.named(value, dependency.name)?.is_some() {
                    let applied =
                        self.schema(self.child(node, dependency.value)?, value, context)?;
                    self.conjunction(state, &applied)?;
                }
            }
        }
        for member in value.members().expect("object") {
            self.graph.tick()?;
            let name = if self.plan.annotations.properties {
                Some(self.units(member.name)?)
            } else {
                None
            };
            let mut selected = FALSE;
            if let Some(properties) = schema.get("properties")
                && let Some(child) = self.named(properties, member.name)?
            {
                let child = self.schema(self.child(node, child)?, member.value, context)?;
                state.valid = self.graph.and(state.valid, child.valid)?;
                selected = TRUE;
                if let Some(name) = &name {
                    self.graph
                        .charge(name.len().saturating_mul(2).saturating_add(256))?;
                    state.properties.insert(name.clone(), TRUE);
                }
            }
            if let Some(patterns) = schema.get("patternProperties").and_then(|v| v.members()) {
                for pattern in patterns {
                    self.graph
                        .charge(pattern.name.text().len().saturating_add(32))?;
                    let keyword = format!("patternProperties/{}", pattern.name.text());
                    let matched =
                        self.pattern(node, &keyword, pattern.name, member.name, context)?;
                    let child =
                        self.schema(self.child(node, pattern.value)?, member.value, context)?;
                    let applies = self.graph.implies(matched, child.valid)?;
                    state.valid = self.graph.and(state.valid, applies)?;
                    selected = self.graph.or(selected, matched)?;
                    if let Some(name) = &name {
                        let old = state.properties.get(name).copied().unwrap_or(FALSE);
                        self.graph
                            .charge(name.len().saturating_mul(2).saturating_add(256))?;
                        state
                            .properties
                            .insert(name.clone(), self.graph.or(old, matched)?);
                    }
                }
            }
            if let Some(child) = schema.get("additionalProperties") {
                let child = self.schema(self.child(node, child)?, member.value, context)?;
                let applies = self.graph.or(selected, child.valid)?;
                state.valid = self.graph.and(state.valid, applies)?;
                if let Some(name) = &name {
                    self.graph
                        .charge(name.len().saturating_mul(2).saturating_add(256))?;
                    state.properties.insert(name.clone(), TRUE);
                }
            }
            if let Some(child) = schema.get("propertyNames") {
                let child = self.schema(self.child(node, child)?, member.name, context)?;
                state.valid = self.graph.and(state.valid, child.valid)?;
            }
            if let Some(name) = &name {
                self.graph.charge(96 + name.len() * 2)?;
            }
        }
        if let Some(child) = schema.get("unevaluatedProperties") {
            let child = self.child(node, child)?;
            for member in value.members().expect("object") {
                let name = self.units(member.name)?;
                let evaluated = state.properties.get(&name).copied().unwrap_or(FALSE);
                let result = self.schema(child, member.value, context)?;
                let valid = self.graph.or(evaluated, result.valid)?;
                state.valid = self.graph.and(state.valid, valid)?;
                state.properties.insert(name, TRUE);
                self.graph.charge(96)?;
            }
        }
        Ok(())
    }
    fn array(
        &mut self,
        node: usize,
        schema: &Interpreted<'_>,
        value: JsonRef<'_>,
        context: &Context,
        state: &mut State,
    ) -> Result<(), NoVerdict> {
        let tuple = schema.profile.edition() != Dialect::Draft202012
            && schema
                .get("items")
                .is_some_and(|v| v.kind() == JsonKind::Array);
        let prefix = schema.get(if tuple { "items" } else { "prefixItems" });
        for (index, item) in value.elements().expect("array").enumerate() {
            self.graph.tick()?;
            if let Some(child) = prefix
                .and_then(|prefix| prefix.element(index))
                .or_else(|| schema.get(if tuple { "additionalItems" } else { "items" }))
            {
                let result = self.schema(self.child(node, child)?, item, context)?;
                state.valid = self.graph.and(state.valid, result.valid)?;
                if self.plan.annotations.items {
                    self.graph.charge(96)?;
                    state.items.insert(index, TRUE);
                }
            }
        }
        if let Some(child) = schema.get("contains") {
            let child = self.child(node, child)?;
            let mut counts = counts::Matches::default();
            for (index, item) in value.elements().expect("array").enumerate() {
                let result = self.schema(child, item, context)?;
                counts.add(result.valid, &mut self.graph)?;
                if self.plan.annotations.items && schema.profile.edition() == Dialect::Draft202012 {
                    let old = state.items.get(&index).copied().unwrap_or(FALSE);
                    self.graph.charge(96)?;
                    state.items.insert(index, self.graph.or(old, result.valid)?);
                }
            }
            let prepared = self.plan.operands.get(&node);
            let mut count = |slot: usize, key: &str, default: u64| -> Result<u64, NoVerdict> {
                let reading = prepared.and_then(|record| record.contains[slot].as_ref());
                if reading.is_none() && schema.get(key).is_none() {
                    return Ok(default);
                }
                self.count_operand(schema, key, reading)?.ok_or_else(|| {
                    no_verdict(
                        NoVerdictReason::UnsupportedCapability,
                        "contains-count-shape",
                        "a contains bound must be a nonnegative integer",
                    )
                })
            };
            let minimum = count(0, "minContains", 1)?;
            let maximum = count(1, "maxContains", u64::MAX)?;
            let valid = counts.within(minimum, maximum, &mut self.graph)?;
            state.valid = self.graph.and(state.valid, valid)?;
        }
        if let Some(child) = schema.get("unevaluatedItems") {
            let child = self.child(node, child)?;
            for (index, item) in value.elements().expect("array").enumerate() {
                let evaluated = state.items.get(&index).copied().unwrap_or(FALSE);
                let result = self.schema(child, item, context)?;
                let valid = self.graph.or(evaluated, result.valid)?;
                state.valid = self.graph.and(state.valid, valid)?;
                self.graph.charge(96)?;
                state.items.insert(index, TRUE);
            }
        }
        Ok(())
    }
    fn origin(
        plan: &Plan,
        atom: &Atom,
        graph: &mut Graph<'_>,
    ) -> Result<Option<SchemaOrigin>, NoVerdict> {
        if atom.node == usize::MAX {
            return Ok(None);
        }
        let node = plan.environment.space().node(atom.node);
        let source = plan.environment.space().source(node.source);
        let location_keyword = if atom.keyword.starts_with("patternProperties/") {
            "patternProperties"
        } else {
            &atom.keyword
        };
        let overhead = source
            .uri
            .as_ref()
            .map_or(0, String::len)
            .saturating_add(location_keyword.len().saturating_mul(2));
        let Some(remaining) = (64usize * 1024).checked_sub(overhead) else {
            return Ok(None);
        };
        // Measure before materializing a potentially enormous original pointer.
        graph.work(64 * 1024)?;
        let Some(pointer_bytes) = backend::location_pointer_size(node.value.view(), remaining / 2)
        else {
            return Ok(None);
        };
        graph.charge(
            pointer_bytes
                .saturating_add(overhead)
                .saturating_mul(16)
                .saturating_add(1024),
        )?;
        let original = plan.environment.space().location(atom.node);
        let bytes = original.pointer.utf16_units().count().saturating_mul(2)
            + original.resource.as_ref().map_or(0, String::len)
            + location_keyword.len().saturating_mul(2);
        if bytes > 64 * 1024 {
            return Ok(None);
        }
        let mut pointer = Vec::with_capacity(bytes.saturating_add(32));
        pointer.extend(original.pointer.utf16_units());
        if atom.keyword == "pattern"
            || matches!(atom.keyword.as_str(), "$id" | "$ref" | "$dynamicRef")
        {
            pointer.push(b'/' as u16);
            pointer.extend(atom.keyword.encode_utf16());
        } else if atom.keyword.starts_with("patternProperties/") {
            // Locate the original keyword without echoing authored regex text.
            pointer.extend("/patternProperties".encode_utf16());
        }
        Ok(Pointer::from_utf16(&pointer)
            .ok()
            .map(|pointer| SchemaOrigin::new(original.resource, pointer)))
    }
    fn conclude(
        &mut self,
        formula: Formula,
        unavailable: &mut Option<String>,
    ) -> Result<ValueOutcome, NoVerdict> {
        if formula == TRUE {
            return Ok(ValueOutcome::Satisfies);
        }
        if formula == FALSE {
            return Ok(super::verdict_only_failure());
        }
        let yes = self.graph.witness(formula)?;
        let negated = self.graph.not(formula)?;
        let no = self.graph.witness(negated)?;
        let support = self.graph.support(formula)?;
        if yes == TRUE && no == TRUE {
            let atom = support
                .iter()
                .map(|&i| &self.atoms[i as usize])
                .find(|a| matches!(a.class, Class::Undefined | Class::PrivateUndefined))
                .expect("dependence needs an undefined choice");
            let mut detail = UndefinedResult::new(
                "semantic-dependence",
                "the value's validity depends on a known undefined schema result",
            );
            detail.location = Self::origin(self.plan, atom, &mut self.graph)?;
            return Ok(ValueOutcome::Undefined { detail });
        }
        let atom = support
            .iter()
            .map(|&i| &self.atoms[i as usize])
            .find(|a| a.class == Class::Inability)
            .or_else(|| {
                support
                    .iter()
                    .map(|&i| &self.atoms[i as usize])
                    .find(|a| a.class == Class::External)
            });
        let (reason, code, message) = match atom.map(|a| a.class) {
            Some(Class::External) => (
                NoVerdictReason::ResourceUnavailable,
                "resource-unavailable",
                "the value is not settled without external schema content",
            ),
            _ => (
                NoVerdictReason::UnsupportedCapability,
                "semantic-analysis-inconclusive",
                "the evaluator has not established the value's result",
            ),
        };
        if reason == NoVerdictReason::ResourceUnavailable
            && let Some(uri) = atom.and_then(|a| a.uri.as_ref())
        {
            self.graph.charge(uri.len().saturating_add(32))?;
            *unavailable = Some(uri.clone());
        }
        let mut detail = atom
            .and_then(|atom| atom.cause.clone())
            .unwrap_or_else(|| no_verdict(reason, code, message));
        detail.location = match atom {
            Some(atom) => Self::origin(self.plan, atom, &mut self.graph)?,
            None => None,
        };
        // Keep missing identity and predicate metadata separate from source
        // origins; diagnostic formatting never needs to echo their text.
        if let Some(atom) = atom {
            let _ = (&atom.uri, &atom.keyword);
        }
        Ok(ValueOutcome::NoVerdict { detail })
    }
}

#[cfg(test)]
mod summary_tests {
    use super::*;

    #[test]
    fn certification_includes_every_evaluated_annotation() {
        let control = std::sync::atomic::AtomicBool::new(false);
        let mut graph = Graph::new(1 << 20, None, 32, &control);
        let (id, cut) = graph.variable(Class::Inability).unwrap();
        let cuts = HashSet::from([id]);
        let mut state = State::truth(TRUE);
        assert!(state.independent(&mut graph, &cuts).unwrap());
        state.properties.insert(vec![b'x' as u16], cut);
        assert!(!state.independent(&mut graph, &cuts).unwrap());
        state.properties.clear();
        state.items.insert(0, cut);
        assert!(!state.independent(&mut graph, &cuts).unwrap());
        state.items.insert(0, FALSE);
        assert!(state.independent(&mut graph, &cuts).unwrap());
        state.valid = cut;
        assert!(!state.independent(&mut graph, &cuts).unwrap());
    }

    fn original_plan(schema: &str, resources: &[(&str, &str)], limits: Limits) -> Plan {
        let document = Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
        ))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
        let external = ExternalSchemas::new(resources.iter().map(|(uri, schema)| ExternalSchema {
            uri: (*uri).into(),
            document: JsonValue::parse(schema).unwrap(),
        }))
        .unwrap();
        let preparation = PreparationLimits::default();
        let control = std::sync::atomic::AtomicBool::new(false);
        let environment = document
            .operation("op")
            .unwrap()
            .input()
            .unwrap()
            .environment(&external, &preparation, &control)
            .unwrap();
        openbindings_internal_schema::preparation::Attempt::run(
            preparation.max_work_units,
            preparation.max_allocation_bytes,
            &control,
            |attempt| Plan::new(environment, limits, preparation, attempt),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn prepared_query_keeps_long_uri_work_out_of_retained_evaluations() {
        let uri = format!("https://example.test/{}", "q".repeat(10_000));
        let schema = serde_json::json!({"$ref": uri}).to_string();
        let plan = original_plan(
            &schema,
            &[(&uri, "true")],
            Limits {
                evaluation_steps: Some(128),
                ..Limits::default()
            },
        );
        let value = JsonValue::parse("1").unwrap();
        let control = std::sync::atomic::AtomicBool::new(false);
        for _ in 0..3 {
            assert!(matches!(
                plan.evaluate(&value, Detail::Outcome, &control),
                ValueOutcome::Satisfies
            ));
        }
        assert!(
            matches!(plan.evaluate(&value, Detail::Outcome, &std::sync::atomic::AtomicBool::new(true)),
            ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
        );
        assert!(matches!(
            plan.evaluate(&value, Detail::Outcome, &control),
            ValueOutcome::Satisfies
        ));
    }

    #[test]
    fn prepared_simple_patterns_do_not_repeat_parser_work() {
        let plan = original_plan(
            r#"{"pattern":"^a+$"}"#,
            &[],
            Limits {
                // Reading this source alone costs over 1,000 work units. A
                // retained call needs only the original predicate and match.
                evaluation_steps: Some(128),
                ..Limits::default()
            },
        );
        assert_eq!(plan.patterns.len(), 1);
        let control = std::sync::atomic::AtomicBool::new(false);
        for _ in 0..3 {
            for (text, valid) in [(r#""aa""#, true), (r#""b""#, false), ("null", true)] {
                let outcome =
                    plan.evaluate(&JsonValue::parse(text).unwrap(), Detail::Outcome, &control);
                assert!(
                    if valid {
                        matches!(outcome, ValueOutcome::Satisfies)
                    } else {
                        matches!(outcome, ValueOutcome::Fails { .. })
                    },
                    "{outcome:?}"
                );
            }
        }
        assert!(
            matches!(plan.evaluate(&JsonValue::parse(r#""a""#).unwrap(), Detail::Outcome, &std::sync::atomic::AtomicBool::new(true)), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
        );
        assert!(matches!(
            plan.evaluate(
                &JsonValue::parse(r#""a""#).unwrap(),
                Detail::Outcome,
                &control
            ),
            ValueOutcome::Satisfies
        ));
    }

    #[test]
    fn prepared_counts_remove_schema_parsing_from_repeated_calls() {
        // Each exact count token costs over 2,000 work units to parse under the
        // original schedule. The prepared calls below have only 128 units.
        let hundred = format!("1e{}2", "0".repeat(253));
        let zero = format!("0.{}", "0".repeat(254));
        let control = std::sync::atomic::AtomicBool::new(false);
        for (schema, value) in [
            (format!(r#"{{"maxLength":{hundred}}}"#), r#""x""#),
            (format!(r#"{{"maxItems":{hundred}}}"#), "[1]"),
            (format!(r#"{{"maxProperties":{hundred}}}"#), r#"{"x":1}"#),
            (
                format!(r#"{{"contains":false,"minContains":{zero},"maxContains":{zero}}}"#),
                "[]",
            ),
        ] {
            let plan = original_plan(
                &schema,
                &[],
                Limits {
                    evaluation_steps: Some(128),
                    ..Limits::default()
                },
            );
            let value = JsonValue::parse(value).unwrap();
            std::thread::scope(|threads| {
                for _ in 0..3 {
                    threads.spawn(|| {
                        assert!(matches!(
                            plan.evaluate(&value, Detail::Outcome, &control),
                            ValueOutcome::Satisfies
                        ));
                    });
                }
            });
            let stopped = std::sync::atomic::AtomicBool::new(true);
            assert!(
                matches!(plan.evaluate(&value, Detail::Outcome, &stopped), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
            );
            assert!(matches!(
                plan.evaluate(&value, Detail::Explain, &control),
                ValueOutcome::Satisfies
            ));
        }
    }

    #[test]
    fn prepared_operand_fallback_preserves_large_unused_and_reached_counts() {
        let control = std::sync::atomic::AtomicBool::new(false);
        let giant = "1".repeat(3_000_000);
        for keyword in ["minimum", "maxLength"] {
            let schema = format!(r#"{{"properties":{{"unused":{{"{keyword}":{giant}}}}}}}"#);
            let plan = original_plan(&schema, &[], Limits::default());
            assert!(matches!(
                plan.evaluate(&JsonValue::parse("{}").unwrap(), Detail::Outcome, &control),
                ValueOutcome::Satisfies
            ));
        }
        let schema = format!(r#"{{"minContains":{giant}}}"#);
        let plan = original_plan(&schema, &[], Limits::default());
        assert!(plan.operands.is_empty());
        assert!(matches!(
            plan.evaluate(&JsonValue::parse("[]").unwrap(), Detail::Outcome, &control),
            ValueOutcome::Satisfies
        ));

        // One byte over the eager-token policy remains a real assertion.
        let bound = format!("1e{}0", "0".repeat(254));
        let plan = original_plan(
            &format!(r#"{{"maxItems":{bound}}}"#),
            &[],
            Limits::default(),
        );
        let outcome = plan.evaluate(
            &JsonValue::parse("[1,2]").unwrap(),
            Detail::Outcome,
            &control,
        );
        assert!(matches!(outcome, ValueOutcome::Fails { .. }), "{outcome:?}");

        // Exhausting the node-record policy must not drop the later predicate.
        let children = std::iter::repeat_n(r#"{"maxItems":3}"#, 1_024)
            .chain(std::iter::once(r#"{"maxItems":0}"#))
            .collect::<Vec<_>>()
            .join(",");
        let plan = original_plan(
            &format!(r#"{{"allOf":[{children}]}}"#),
            &[],
            Limits::default(),
        );
        assert_eq!(plan.operands.len(), 1_024);
        let outcome = plan.evaluate(&JsonValue::parse("[1]").unwrap(), Detail::Outcome, &control);
        assert!(matches!(outcome, ValueOutcome::Fails { .. }), "{outcome:?}");
    }

    #[test]
    fn prepared_operand_aggregate_fallback_and_comparison_costs_remain_real() {
        let hundred = format!("1e{}2", "0".repeat(253));
        let zero = format!("0.{}", "0".repeat(254));
        let children = std::iter::repeat_n(format!(r#"{{"maxItems":{hundred}}}"#), 256)
            .chain(std::iter::once(format!(r#"{{"maxItems":{zero}}}"#)))
            .collect::<Vec<_>>()
            .join(",");
        let plan = original_plan(
            &format!(r#"{{"allOf":[{children}]}}"#),
            &[],
            Limits::default(),
        );
        let control = std::sync::atomic::AtomicBool::new(false);
        let outcome = plan.evaluate(&JsonValue::parse("[1]").unwrap(), Detail::Outcome, &control);
        assert!(matches!(outcome, ValueOutcome::Fails { .. }), "{outcome:?}");

        // This token is cached, but comparing its exponent still needs the
        // schema-dependent magnitude allowance. A tiny work cap cannot fund it.
        let source = format!(r#"{{"minimum":1e{}}}"#, "9".repeat(200));
        let value = JsonValue::parse("1").unwrap();
        let plan = original_plan(
            &source,
            &[],
            Limits {
                evaluation_steps: Some(128),
                ..Limits::default()
            },
        );
        assert!(matches!(
            plan.operands.get(&plan.entry).unwrap().numbers[0].reading,
            operands::Reading::Parsed(_)
        ));
        assert!(
            matches!(plan.evaluate(&value, Detail::Outcome, &control), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded)
        );
        let funded = original_plan(&source, &[], Limits::default());
        assert!(matches!(
            funded.evaluate(&value, Detail::Outcome, &control),
            ValueOutcome::Fails { .. }
        ));
    }

    #[test]
    fn ordinary_large_containers_keep_no_unused_annotation_or_leaf_states() {
        let control = std::sync::atomic::AtomicBool::new(false);
        let array = original_plan(
            r#"{"type":"array","items":{"type":"integer","minimum":0}}"#,
            &[],
            Limits::default(),
        );
        let valid = serde_json::Value::Array((0..20_000).map(serde_json::Value::from).collect());
        let value = JsonValue::parse(valid.to_string()).unwrap();
        let outcome = array.evaluate(&value, Detail::Outcome, &control);
        assert!(matches!(outcome, ValueOutcome::Satisfies), "{outcome:?}");
        let mut invalid = valid;
        invalid[19_999] = serde_json::Value::from(-1);
        let outcome = array.evaluate(
            &JsonValue::parse(invalid.to_string()).unwrap(),
            Detail::Outcome,
            &control,
        );
        assert!(matches!(outcome, ValueOutcome::Fails { .. }), "{outcome:?}");

        let object = original_plan(
            r#"{"type":"object","additionalProperties":{"type":"integer"}}"#,
            &[],
            Limits::default(),
        );
        let fields: serde_json::Map<String, serde_json::Value> = (0..50_000)
            .map(|i| (format!("key{i}"), serde_json::Value::from(i)))
            .collect();
        let value = JsonValue::parse(serde_json::Value::Object(fields).to_string()).unwrap();
        let outcome = object.evaluate(&value, Detail::Outcome, &control);
        assert!(matches!(outcome, ValueOutcome::Satisfies), "{outcome:?}");
    }

    #[test]
    fn annotation_demand_preserves_local_selection_counts_and_consumers() {
        let control = std::sync::atomic::AtomicBool::new(false);
        for (schema, value, valid) in [
            (
                r#"{"patternProperties":{"^a":{"type":"integer"}},"additionalProperties":false}"#,
                r#"{"a":1}"#,
                true,
            ),
            (
                r#"{"patternProperties":{"^a":{"type":"integer"}},"additionalProperties":false}"#,
                r#"{"b":1}"#,
                false,
            ),
            (
                r#"{"contains":{"const":1},"minContains":2,"maxContains":2}"#,
                "[1,2,1]",
                true,
            ),
            (
                r#"{"contains":{"const":1},"minContains":2,"maxContains":2}"#,
                "[1,2,2]",
                false,
            ),
            (
                r#"{"allOf":[{"properties":{"a":true}}],"unevaluatedProperties":false}"#,
                r#"{"a":1}"#,
                true,
            ),
            (
                r#"{"allOf":[{"properties":{"a":true}}],"unevaluatedProperties":false}"#,
                r#"{"a":1,"b":2}"#,
                false,
            ),
            (
                r#"{"allOf":[{"contains":{"const":1}}],"unevaluatedItems":false}"#,
                "[1,1]",
                true,
            ),
            (
                r#"{"allOf":[{"contains":{"const":1}}],"unevaluatedItems":false}"#,
                "[1,2]",
                false,
            ),
        ] {
            let plan = original_plan(schema, &[], Limits::default());
            let outcome =
                plan.evaluate(&JsonValue::parse(value).unwrap(), Detail::Outcome, &control);
            assert!(
                if valid {
                    matches!(outcome, ValueOutcome::Satisfies)
                } else {
                    matches!(outcome, ValueOutcome::Fails { .. })
                },
                "{schema} {value}: {outcome:?}"
            );
        }
    }

    #[test]
    fn empty_custom_applicators_are_not_classified_as_leaves() {
        let meta = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/applicator":true,"https://json-schema.org/draft/2020-12/vocab/validation":true},"type":"object"}"#;
        let control = std::sync::atomic::AtomicBool::new(false);
        for (keyword, valid) in [("anyOf", false), ("oneOf", false), ("allOf", true)] {
            let consumer =
                serde_json::json!({"$schema":"https://fixture.test/meta",keyword:[]}).to_string();
            let plan = original_plan(
                r#"{"$ref":"https://fixture.test/schema"}"#,
                &[
                    ("https://fixture.test/meta", meta),
                    ("https://fixture.test/schema", &consumer),
                ],
                Limits::default(),
            );
            for value in ["null", "false", "0", r#""x""#, "[]", "{}"] {
                let outcome =
                    plan.evaluate(&JsonValue::parse(value).unwrap(), Detail::Outcome, &control);
                assert!(
                    if valid {
                        matches!(outcome, ValueOutcome::Satisfies)
                    } else {
                        matches!(outcome, ValueOutcome::Fails { .. })
                    },
                    "{keyword} {value}: {outcome:?}"
                );
            }
        }
    }

    #[test]
    fn recursive_support_keeps_foreign_leaf_absence_without_leaf_memo() {
        let control = std::sync::atomic::AtomicBool::new(false);
        for dialect in [
            "https://json-schema.org/draft/2019-09/schema",
            "https://json-schema.org/draft/2020-12/schema",
        ] {
            let resource = serde_json::json!({
                "$id":"https://compact.test/A",
                "allOf":[{"$ref":"#"},{"$id":"leaf","$schema":dialect,"type":"object"}]
            })
            .to_string();
            let plan = original_plan(
                r#"{"allOf":[{"$ref":"https://compact.test/A"}],"unevaluatedProperties":false}"#,
                &[("https://compact.test/A", &resource)],
                Limits::default(),
            );
            let outcome = plan.evaluate(
                &JsonValue::parse(r#"{"a":1}"#).unwrap(),
                Detail::Outcome,
                &control,
            );
            assert!(
                matches!(outcome, ValueOutcome::Fails { .. }),
                "{dialect}: {outcome:?}"
            );
        }
    }

    #[test]
    fn prepared_pattern_readings_keep_local_outcomes_and_call_owned_vm_state() {
        let plan = original_plan(
            r#"{"patternProperties":{"^(a|b)+$":{"type":"integer"}},"additionalProperties":false}"#,
            &[],
            Limits::default(),
        );
        std::thread::scope(|scope| {
            for (text, valid) in [
                (r#"{"ab":1}"#, true),
                (r#"{"ab":false}"#, false),
                (r#"{"c":1}"#, false),
            ] {
                let plan = &plan;
                scope.spawn(move || {
                    let value = JsonValue::parse(text).unwrap();
                    let control = std::sync::atomic::AtomicBool::new(false);
                    for _ in 0..3 {
                        let outcome = plan.evaluate(&value, Detail::Outcome, &control);
                        assert!(
                            if valid {
                                matches!(outcome, ValueOutcome::Satisfies)
                            } else {
                                matches!(outcome, ValueOutcome::Fails { .. })
                            },
                            "{outcome:?}"
                        );
                    }
                });
            }
        });
        for (pattern, undefined) in [("[", true), (r"(a)\1", false)] {
            let schema = serde_json::json!({"pattern":pattern}).to_string();
            let plan = original_plan(&schema, &[], Limits::default());
            let control = std::sync::atomic::AtomicBool::new(false);
            let outcome = plan.evaluate(
                &JsonValue::parse(r#""aa""#).unwrap(),
                Detail::Outcome,
                &control,
            );
            assert!(if undefined {
                matches!(outcome, ValueOutcome::Undefined { .. })
            } else {
                matches!(outcome, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::UnsupportedCapability)
            });
            assert!(matches!(
                plan.evaluate(&JsonValue::parse("1").unwrap(), Detail::Outcome, &control),
                ValueOutcome::Satisfies
            ));
            let plan = original_plan(
                &serde_json::json!({"anyOf":[{"pattern":pattern},true]}).to_string(),
                &[],
                Limits::default(),
            );
            assert!(matches!(
                plan.evaluate(
                    &JsonValue::parse(r#""aa""#).unwrap(),
                    Detail::Outcome,
                    &control
                ),
                ValueOutcome::Satisfies
            ));
        }
    }

    #[test]
    fn prepared_queries_preserve_original_bases_and_typed_missing_outcomes() {
        let plan = original_plan(
            r#"{"anyOf":[{"$ref":"https://example.test/a/entry"},{"$ref":"https://example.test/b/entry"}]}"#,
            &[
                ("https://example.test/a/entry", r#"{"$ref":"child"}"#),
                ("https://example.test/b/entry", r#"{"$ref":"child"}"#),
                ("https://example.test/a/child", r#"{"type":"string"}"#),
                ("https://example.test/b/child", r#"{"type":"integer"}"#),
            ],
            Limits::default(),
        );
        let control = std::sync::atomic::AtomicBool::new(false);
        for (text, valid) in [(r#""a""#, true), ("1", true), ("false", false)] {
            let result = plan.evaluate(&JsonValue::parse(text).unwrap(), Detail::Outcome, &control);
            assert!(
                if valid {
                    matches!(result, ValueOutcome::Satisfies)
                } else {
                    matches!(result, ValueOutcome::Fails { .. })
                },
                "{result:?}"
            );
        }
        let missing = r#"{"$ref":"https://example.test/missing"}"#;
        let plan = original_plan(missing, &[], Limits::default());
        assert!(
            matches!(plan.evaluate(&JsonValue::parse("1").unwrap(), Detail::Outcome, &control),
            ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::ResourceUnavailable)
        );
        let independent = original_plan(
            &format!(r#"{{"anyOf":[true,{missing}]}}"#),
            &[],
            Limits::default(),
        );
        assert!(matches!(
            independent.evaluate(&JsonValue::parse("1").unwrap(), Detail::Outcome, &control),
            ValueOutcome::Satisfies
        ));
    }
}
