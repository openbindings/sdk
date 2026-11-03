//! Per-value dependence analysis over original schema locations.
//! The ordinary compiled engine remains the fast path. This layer represents
//! local effects and applicator annotations together when that path cannot
//! establish a value's result. It does not infer Undefined from failed bounds.
mod decision;
mod preparation;
mod values;
use self::decision::{Class, FALSE, Formula, Graph, TRUE};
use super::{Compiled, DefaultEvaluator, Limits, no_verdict};
use openbindings::*;
use openbindings_internal_json::{backend, decimal::Decimal};
use openbindings_internal_schema::{
    OriginalEnvironment, QueryFailure, SchemaSpace, dialect::Dialect,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
};

type Primitive = Result<Option<Arc<Compiled>>, NoVerdict>;
const RECURSIVE_SCOPE: &str = "#recursive-scope";

#[derive(Clone, Copy)]
struct Interpreted<'a> {
    value: JsonRef<'a>,
    dialect: Dialect,
}
impl<'a> Interpreted<'a> {
    fn get(self, keyword: &str) -> Option<JsonRef<'a>> {
        if !self.dialect.active(keyword)
            || (self.dialect.reference_object(self.value) && keyword != "$ref")
        {
            None
        } else {
            self.value.get(keyword)
        }
    }
}

pub(super) struct Plan {
    environment: SchemaEnvironment,
    limits: Limits,
    preparation: PreparationLimits,
    primitives: Mutex<HashMap<usize, Primitive>>,
}
impl Plan {
    pub(super) fn new(
        environment: SchemaEnvironment,
        limits: Limits,
        preparation: PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self, PreparationFailure> {
        preparation::check(&environment, &limits, control)?;
        Ok(Self {
            environment,
            limits,
            preparation,
            primitives: Mutex::new(HashMap::new()),
        })
    }
    fn primitive(
        &self,
        node: usize,
        control: &dyn WorkControl,
    ) -> Result<Option<Arc<Compiled>>, NoVerdict> {
        const PRIMITIVES: &[&str] = &[
            "type",
            "enum",
            "const",
            "minimum",
            "maximum",
            "exclusiveMinimum",
            "exclusiveMaximum",
            "multipleOf",
            "minLength",
            "maxLength",
            "minItems",
            "maxItems",
            "uniqueItems",
            "minProperties",
            "maxProperties",
            "required",
            "dependentRequired",
        ];
        let mut cache = self.primitives.lock().map_err(|_| {
            no_verdict(
                NoVerdictReason::EvaluatorFailure,
                "semantic-cache",
                "semantic predicate cache was interrupted",
            )
        })?;
        if let Some(known) = cache.get(&node) {
            return known.clone();
        }
        if cache.len() >= self.limits.program_bytes / 8192 {
            return Err(decision::limit("semantic-predicate-memory-limit"));
        }
        let original = self.environment.space().node(node);
        let schema = Interpreted {
            value: original.value.view(),
            dialect: original.dialect.expect("known primitive profile"),
        };
        let mut fields: Vec<_> = PRIMITIVES
            .iter()
            .filter_map(|name| schema.get(name).map(|v| format!("\"{name}\":{}", v.text())))
            .collect();
        if let Some(dependencies) = schema.get("dependencies").and_then(|v| v.members()) {
            let fields_for_arrays: Vec<_> = dependencies
                .filter(|d| d.value.kind() == JsonKind::Array)
                .map(|d| format!("{}:{}", d.name.text(), d.value.text()))
                .collect();
            if !fields_for_arrays.is_empty() {
                fields.push(format!(
                    "\"dependentRequired\":{{{}}}",
                    fields_for_arrays.join(",")
                ));
            }
        }
        let prepared = if fields.is_empty() {
            Ok(None)
        } else {
            let body = format!("{{{}}}", fields.join(","));
            let text = [
                "{\"openbindings\":\"0.2.0\",\"operations\":{\"leaf\":{\"input\":",
                &body,
                "}}}",
            ]
            .concat();
            (|| {
                let doc = Document::parse(text)
                    .map_err(|_| decision::limit("semantic-predicate-input-limit"))?;
                let validation = doc.validate().map_err(|_| {
                    no_verdict(
                        NoVerdictReason::UnsupportedCapability,
                        "semantic-predicate-schema",
                        "a primitive predicate cannot be interpreted",
                    )
                })?;
                let proof = validation.conformant().ok_or_else(|| {
                    no_verdict(
                        NoVerdictReason::UnsupportedCapability,
                        "semantic-predicate-schema",
                        "a primitive predicate does not have a supported schema shape",
                    )
                })?;
                let schema = proof
                    .resolve_operation("leaf")
                    .expect("leaf operation")
                    .input()
                    .expect("leaf schema");
                let external = ExternalSchemas::default();
                let evaluator = DefaultEvaluator::with_limits(self.limits.clone());
                evaluator
                    .compiled(
                        EvaluationRequest {
                            schema: &schema,
                            external_schemas: &external,
                        },
                        &self.preparation,
                        control,
                    )
                    .map(Some)
                    .map_err(preparation_inability)
            })()
        };
        // Interruption belongs to this attempt, never to the prepared schema.
        if prepared.is_ok() {
            cache.insert(node, prepared.clone());
        }
        prepared
    }
    pub(super) fn evaluate(
        &self,
        value: &JsonValue,
        _detail: Detail,
        control: &dyn WorkControl,
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
            graph: Graph::new(
                self.limits.program_bytes,
                self.limits.evaluation_steps,
                depth,
                control,
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
            depth: 0,
            maximum_depth: depth,
            control,
        };
        loop {
            let result = run.schema(self.environment.entry(), value.view(), &Vec::new());
            if run.retry {
                run.retry = false;
                if !run.finish_attempt() {
                    return ValueOutcome::NoVerdict {
                        detail: cycle_inability(),
                    };
                }
                continue;
            }
            let result = result.and_then(|state| {
                if run.pending_cycles.is_empty()
                    || independent(state.valid, &mut run.graph, &run.cuts)?
                {
                    return run.conclude(state.valid).map(Some);
                }
                // An enclosing applicator may eliminate a provisional cut. Only
                // make unresolved components opaque after completing that proof
                // attempt. Certified summaries remain valid across retries.
                if run.finish_attempt() {
                    Ok(None)
                } else {
                    Err(cycle_inability())
                }
            });
            match result {
                Ok(Some(outcome)) => return outcome,
                Ok(None) => {}
                Err(detail) => return ValueOutcome::NoVerdict { detail },
            }
        }
    }
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
    depth: usize,
    maximum_depth: usize,
    control: &'a dyn WorkControl,
}
impl Run<'_> {
    fn finish_attempt(&mut self) -> bool {
        let mut progress = false;
        for visit in self.pending_cycles.drain() {
            if !self.memo.contains_key(&visit) {
                progress |= self.opaque_cycles.insert(visit);
            }
        }
        self.provisional.clear();
        progress
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
    fn enter(&self, node: usize, context: &Context) -> Context {
        let resource = self.space().resource(self.space().node(node).resource);
        let mut out: BTreeMap<_, _> = context.iter().cloned().collect();
        for (name, anchors) in &resource.anchors {
            if anchors.iter().any(|(_, dynamic)| *dynamic) {
                out.entry(name.clone()).or_insert_with(|| {
                    if anchors.len() == 1 {
                        Some(anchors[0].0)
                    } else {
                        None
                    }
                });
            }
        }
        let original = self.space().node(node);
        if original.dialect == Some(Dialect::Draft201909)
            && original
                .value
                .get("$recursiveAnchor")
                .and_then(|v| v.as_bool())
                == Some(true)
        {
            out.entry(RECURSIVE_SCOPE.into()).or_insert(resource.root);
        }
        out.into_iter().collect()
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
            uri.clone(),
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
        let mut state = self.effect(class, node, keyword, value, context, uri.clone())?;
        let valid = state.valid;
        // Unknown schema contents may produce annotations. This is a conservative
        // enlargement, so its variables cannot witness an Undefined conclusion.
        // A reference's possible annotations follow its same validity choice.
        // Finite property/index selections are coherent across repeated calls.
        if let Some(members) = value.members() {
            for member in members {
                let name = member.name.utf16_units().expect("member string");
                let selected = self.atom(
                    class,
                    node,
                    format!("{keyword}/annotation"),
                    Subject::Member(identity, name.clone()),
                    context,
                    uri.clone(),
                )?;
                let selected = self.graph.and(valid, selected)?;
                self.graph.charge(96 + name.len() * 2)?;
                state.properties.insert(name, selected);
            }
        }
        if let Some(elements) = value.elements() {
            for (index, _) in elements.enumerate() {
                let selected = self.atom(
                    class,
                    node,
                    format!("{keyword}/annotation/{index}"),
                    Subject::Instance(identity),
                    context,
                    uri.clone(),
                )?;
                state.items.insert(index, self.graph.and(valid, selected)?);
                self.graph.charge(96)?;
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
        let mut context = incoming.clone();
        let mut path = Vec::<(usize, Context, Option<String>)>::new();
        loop {
            self.graph.tick()?;
            context = self.enter(node, &context);
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
                || original.dialect.is_none()
                || self.space().query_inventory(original.resource).is_err()
                || schema
                    .get("$id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| !openbindings_internal_schema::uri::valid(id))
            {
                return Ok((node, context, false));
            }
            let dialect = original.dialect.expect("known alias profile");
            let mut alias = None;
            let Some(fields) = schema.view().members() else {
                return Ok((node, context, false));
            };
            for field in fields {
                if field.name.as_str().is_some_and(|key| {
                    !dialect.active(key)
                        || (dialect.reference_object(schema.view()) && key != "$ref")
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
                        | "format" | "contentEncoding" | "contentMediaType" | "contentSchema",
                    ) => {}
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
                let Ok(target) = self.space().query(node, reference) else {
                    return Ok((node, context, false));
                };
                let dynamic = if keyword == "$recursiveRef" && self.recursive_target(target.node) {
                    Some(RECURSIVE_SCOPE.into())
                } else {
                    (keyword == "$dynamicRef")
                        .then_some(target.dynamic_name)
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
        let key = Visit {
            schema: node,
            instance: backend::ref_id(value),
            context: context.clone(),
        };
        if let Some(known) = self.memo.get(&key) {
            return Ok(known.clone());
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
                self.graph.charge(256)?;
                self.pending_cycles.insert(visit.clone());
            }
            // A cut is provisional, never the meaning of a recursive body. Its
            // namespace differs from the persistent opaque fallback summary.
            // Parent frames may eliminate every cut before certification.
            let first = self.atoms.len();
            let result =
                self.unknown_schema(Class::Inability, node, "$cycle-cut", value, &context, None)?;
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
        self.active.push(key.clone());
        self.graph
            .charge(256 + context.iter().map(|(s, _)| s.len() + 32).sum::<usize>())?;
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
        if result.independent(&mut self.graph, &self.cuts)? {
            self.memo.insert(key, result.clone());
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
        let Some(dialect) = self.space().node(node).dialect else {
            return self.unknown_schema(Class::Inability, node, "$dialect", value, context, None);
        };
        let schema = Interpreted {
            value: schema.view(),
            dialect,
        };
        if dialect.reference_object(schema.value) {
            return self.reference(
                node,
                "$ref",
                schema.get("$ref").expect("reference object"),
                value,
                context,
            );
        }
        let mut state = State::truth(TRUE);
        if schema
            .get("$id")
            .and_then(|id| id.as_str())
            .is_some_and(|id| !openbindings_internal_schema::uri::valid(id))
        {
            let effect = self.effect(Class::Undefined, node, "$id", value, &Vec::new(), None)?;
            self.conjunction(&mut state, &effect)?;
        }
        match self.plan.primitive(node, self.control) {
            Ok(Some(compiled)) => {
                match compiled.evaluate(&value.to_owned(), Detail::Outcome, self.control) {
                    ValueOutcome::Satisfies => {}
                    ValueOutcome::Fails { .. } => state.valid = FALSE,
                    ValueOutcome::NoVerdict { detail }
                        if matches!(
                            detail.reason,
                            NoVerdictReason::LimitExceeded | NoVerdictReason::Cancelled
                        ) =>
                    {
                        return Err(detail);
                    }
                    ValueOutcome::NoVerdict { detail } => {
                        let unknown = self.effect(
                            Class::Inability,
                            node,
                            "$predicate",
                            value,
                            context,
                            None,
                        )?;
                        self.cause(unknown.valid, detail)?;
                        self.conjunction(&mut state, &unknown)?;
                    }
                    ValueOutcome::Undefined { .. } => {
                        return Err(no_verdict(
                            NoVerdictReason::EvaluatorFailure,
                            "primitive-undefined",
                            "a defined primitive predicate unexpectedly returned undefined",
                        ));
                    }
                }
            }
            Ok(None) => {}
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
        if let Some(pattern) = schema.get("pattern")
            && value.kind() == JsonKind::String
        {
            let matched = self.pattern(node, "pattern", pattern, value, context)?;
            state.valid = self.graph.and(state.valid, matched)?;
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
        target.dialect == Some(Dialect::Draft201909)
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
            return self.effect(Class::Inability, node, keyword, value, context, None);
        };
        if keyword == "$recursiveRef" && reference != "#" {
            return self.effect(Class::Inability, node, keyword, value, context, None);
        }
        match self.space().query(node, reference) {
            Ok(target) => {
                let recursive = keyword == "$recursiveRef" && self.recursive_target(target.node);
                let selected = if keyword == "$dynamicRef" || recursive {
                    if let Some(name) = if recursive {
                        Some(RECURSIVE_SCOPE.into())
                    } else {
                        target.dynamic_name
                    } {
                        match context.iter().find(|(binding, _)| *binding == name) {
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
                let predicate = format!("$external:{uri}#{fragment}");
                self.unknown_schema(
                    Class::External,
                    usize::MAX,
                    &predicate,
                    value,
                    context,
                    Some(uri),
                )
            }
            Err(
                QueryFailure::IdentityConflict
                | QueryFailure::UnsupportedDialect(_)
                | QueryFailure::UnsupportedCapability(_),
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
        self.graph.charge(value.text().len().saturating_mul(2))?;
        let units = value.utf16_units().expect("pattern string");
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
        let Some(source) = pattern.as_str() else {
            return self.pattern_atom(Class::Inability, node, keyword, value);
        };
        if source.len() > self.plan.limits.pattern_bytes {
            return Err(decision::limit("pattern-byte-limit"));
        }
        let reading = super::pattern::read(
            source,
            self.plan.limits.pattern_depth,
            self.plan.limits.pattern_bytes,
        );
        match reading {
            Err(super::pattern::Refusal::Invalid)
            | Ok(super::pattern::Reading::Unsupported(super::pattern::Unsupported::Invalid)) => {
                self.pattern_atom(Class::Undefined, node, keyword, value)
            }
            Err(super::pattern::Refusal::Depth) => Err(decision::limit("pattern-depth-limit")),
            Ok(super::pattern::Reading::Unsupported(
                super::pattern::Unsupported::Size | super::pattern::Unsupported::Memory(_),
            )) => Err(decision::limit("pattern-size-limit")),
            Ok(super::pattern::Reading::Unsupported(why)) => {
                let formula = self.pattern_atom(Class::Inability, node, keyword, value)?;
                self.cause(formula, super::unsupported_pattern(why, "pattern"))?;
                Ok(formula)
            }
            Ok(super::pattern::Reading::Linear(linear)) => {
                let Some(text) = value.as_str() else {
                    let formula = self.pattern_atom(Class::Inability, node, keyword, value)?;
                    self.cause(
                        formula,
                        no_verdict(
                            NoVerdictReason::UnsupportedCapability,
                            "lone-surrogate-instance",
                            "the pattern matcher cannot interpret an unpaired UTF-16 code unit",
                        ),
                    )?;
                    return Ok(formula);
                };
                self.graph.charge(linear.form.len())?;
                let matched = if let Some(simple) = linear.simple {
                    simple.is_match(text)
                } else {
                    let allowance = super::pattern::Allowance {
                        compiled: self
                            .plan
                            .limits
                            .total_pattern_memory
                            .min(super::pattern::COMPILED_SIZE),
                        cache: self
                            .plan
                            .limits
                            .total_pattern_memory
                            .min(super::pattern::SEARCH_CACHE),
                    };
                    let regex = super::pattern::compile(&linear.form, allowance)
                        .ok_or_else(|| decision::limit("pattern-size-limit"))?;
                    regex.is_match(text)
                };
                Ok(if matched { TRUE } else { FALSE })
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
            let name = member.name.utf16_units().expect("member name");
            let mut selected = FALSE;
            if let Some(properties) = schema.get("properties")
                && let Some(child) = self.named(properties, member.name)?
            {
                let child = self.schema(self.child(node, child)?, member.value, context)?;
                state.valid = self.graph.and(state.valid, child.valid)?;
                selected = TRUE;
                state.properties.insert(name.clone(), TRUE);
            }
            if let Some(patterns) = schema.get("patternProperties").and_then(|v| v.members()) {
                for pattern in patterns {
                    let keyword = format!("patternProperties/{}", pattern.name.text());
                    let matched =
                        self.pattern(node, &keyword, pattern.name, member.name, context)?;
                    let child =
                        self.schema(self.child(node, pattern.value)?, member.value, context)?;
                    let applies = self.graph.implies(matched, child.valid)?;
                    state.valid = self.graph.and(state.valid, applies)?;
                    selected = self.graph.or(selected, matched)?;
                    let old = state.properties.get(&name).copied().unwrap_or(FALSE);
                    state
                        .properties
                        .insert(name.clone(), self.graph.or(old, matched)?);
                }
            }
            if let Some(child) = schema.get("additionalProperties") {
                let child = self.schema(self.child(node, child)?, member.value, context)?;
                let applies = self.graph.or(selected, child.valid)?;
                state.valid = self.graph.and(state.valid, applies)?;
                state.properties.insert(name.clone(), TRUE);
            }
            if let Some(child) = schema.get("propertyNames") {
                let child = self.schema(self.child(node, child)?, member.name, context)?;
                state.valid = self.graph.and(state.valid, child.valid)?;
            }
            self.graph.charge(96 + name.len() * 2)?;
        }
        if let Some(child) = schema.get("unevaluatedProperties") {
            let child = self.child(node, child)?;
            for member in value.members().expect("object") {
                let name = member.name.utf16_units().expect("name");
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
        let tuple = schema.dialect != Dialect::Draft202012
            && schema
                .get("items")
                .is_some_and(|v| v.kind() == JsonKind::Array);
        let prefix: Vec<_> = schema
            .get(if tuple { "items" } else { "prefixItems" })
            .and_then(|v| v.elements())
            .into_iter()
            .flatten()
            .collect();
        for (index, item) in value.elements().expect("array").enumerate() {
            self.graph.tick()?;
            if let Some(child) = prefix
                .get(index)
                .copied()
                .or_else(|| schema.get(if tuple { "additionalItems" } else { "items" }))
            {
                let result = self.schema(self.child(node, child)?, item, context)?;
                state.valid = self.graph.and(state.valid, result.valid)?;
                state.items.insert(index, TRUE);
                self.graph.charge(96)?;
            }
        }
        if let Some(child) = schema.get("contains") {
            let child = self.child(node, child)?;
            let mut counts = vec![TRUE];
            for (index, item) in value.elements().expect("array").enumerate() {
                let result = self.schema(child, item, context)?;
                let not = self.graph.not(result.valid)?;
                let mut next = vec![FALSE; counts.len() + 1];
                self.graph.charge(next.len() * 8)?;
                for (count, &formula) in counts.iter().enumerate() {
                    let same = self.graph.and(formula, not)?;
                    next[count] = self.graph.or(next[count], same)?;
                    let more = self.graph.and(formula, result.valid)?;
                    next[count + 1] = self.graph.or(next[count + 1], more)?;
                }
                counts = next;
                if schema.dialect == Dialect::Draft202012 {
                    let old = state.items.get(&index).copied().unwrap_or(FALSE);
                    state.items.insert(index, self.graph.or(old, result.valid)?);
                }
            }
            let count = |key: &str, default: u64| {
                schema
                    .get(key)
                    .and_then(|n| n.number_text())
                    .and_then(|n| Decimal::parse(n).nonnegative_integer_u64_saturating())
                    .unwrap_or(default)
            };
            let minimum = count("minContains", 1);
            let maximum = count("maxContains", u64::MAX);
            let mut valid = FALSE;
            for (count, &formula) in counts.iter().enumerate() {
                if (minimum..=maximum).contains(&(count as u64)) {
                    valid = self.graph.or(valid, formula)?;
                }
            }
            state.valid = self.graph.and(state.valid, valid)?;
        }
        if let Some(child) = schema.get("unevaluatedItems") {
            let child = self.child(node, child)?;
            for (index, item) in value.elements().expect("array").enumerate() {
                let evaluated = state.items.get(&index).copied().unwrap_or(FALSE);
                let result = self.schema(child, item, context)?;
                let valid = self.graph.or(evaluated, result.valid)?;
                state.valid = self.graph.and(state.valid, valid)?;
                state.items.insert(index, TRUE);
                self.graph.charge(96)?;
            }
        }
        Ok(())
    }
    fn origin(&self, atom: &Atom) -> Option<SchemaOrigin> {
        if atom.node == usize::MAX {
            return None;
        }
        let node = self.space().node(atom.node);
        let source = self.space().source(node.source);
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
        let remaining = (64usize * 1024).checked_sub(overhead)?;
        // Measure before materializing a potentially enormous original pointer.
        backend::location_pointer_size(node.value.view(), remaining / 2)?;
        let original = self.space().location(atom.node);
        let bytes = original.pointer.utf16_units().count().saturating_mul(2)
            + original.resource.as_ref().map_or(0, String::len)
            + location_keyword.len().saturating_mul(2);
        if bytes > 64 * 1024 {
            return None;
        }
        let mut pointer: Vec<u16> = original.pointer.utf16_units().collect();
        if atom.keyword == "pattern"
            || matches!(atom.keyword.as_str(), "$id" | "$ref" | "$dynamicRef")
        {
            pointer.push(b'/' as u16);
            pointer.extend(atom.keyword.encode_utf16());
        } else if atom.keyword.starts_with("patternProperties/") {
            // Locate the original keyword without echoing authored regex text.
            pointer.extend("/patternProperties".encode_utf16());
        }
        Some(SchemaOrigin::new(
            original.resource,
            Pointer::from_utf16(&pointer).ok()?,
        ))
    }
    fn conclude(&mut self, formula: Formula) -> Result<ValueOutcome, NoVerdict> {
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
            detail.location = self.origin(atom);
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
        let mut detail = atom
            .and_then(|atom| atom.cause.clone())
            .unwrap_or_else(|| no_verdict(reason, code, message));
        detail.location = atom.and_then(|atom| self.origin(atom));
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
}
