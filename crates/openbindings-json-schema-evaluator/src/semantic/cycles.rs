//! Narrow original-graph certificates for direct recursive dependence.
//!
//! These certificates are root conclusions, never reusable Boolean effects for
//! component members. They run only after ordinary universal S/F proofs fail.
//! A traversal cut, unfinished closure or exhausted budget is not a certificate.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
    Reference,
    All,
    Any,
    One,
    Not,
}
struct Shape {
    operator: Operator,
    operand: JsonValue,
}

impl Run<'_> {
    /// Inspect each original node, including every transparent wrapper. Generic
    /// alias normalization also accepts dynamic dispatch and foreign dialects;
    /// it must not erase those exclusions before this narrower proof inspects them.
    fn cycle_shape(&mut self, node: usize) -> Result<Option<Shape>, NoVerdict> {
        self.graph.tick()?;
        let original = self.space().node(node);
        if original.profile != Some(Profile::standard(Dialect::Draft202012))
            || !self.space().eligible(original.resource)
            || self.space().query_inventory(original.resource).is_err()
        {
            return Ok(None);
        }
        let schema = original.value.clone();
        if schema.get("$id").is_some_and(|id| {
            id.as_str()
                .is_none_or(|text| !openbindings_internal_schema::uri::valid(text))
        }) {
            return Ok(None);
        }
        let Some(fields) = schema.view().members() else {
            return Ok(None);
        };
        let mut shape = None;
        for field in fields {
            self.graph.work(field.name.text().len().saturating_add(1))?;
            let operator = match field.name.as_str() {
                Some("$ref") => Operator::Reference,
                Some("allOf") => Operator::All,
                Some("anyOf") => Operator::Any,
                Some("oneOf") => Operator::One,
                Some("not") => Operator::Not,
                Some(
                    "$schema" | "$id" | "$anchor" | "$defs" | "definitions" | "$comment" | "title"
                    | "description" | "default" | "examples" | "deprecated" | "readOnly"
                    | "writeOnly" | "contentEncoding" | "contentMediaType" | "contentSchema"
                    | "format",
                ) => continue,
                _ => return Ok(None),
            };
            if shape.is_some() {
                return Ok(None);
            }
            self.graph.charge(128)?;
            shape = Some(Shape {
                operator,
                operand: field.value.to_owned(),
            });
        }
        Ok(shape)
    }

    fn static_cycle_entry(
        &mut self,
        mut node: usize,
        instance: usize,
        incoming: &Context,
        unavailable: &mut Option<String>,
    ) -> Result<Option<Visit>, NoVerdict> {
        self.context_copy(incoming)?;
        let mut context = incoming.clone();
        let mut seen = HashSet::new();
        loop {
            self.graph.tick()?;
            context = self.enter(node, &context)?;
            // This first certificate supports no dynamic or recursive scope.
            // Even an off-path dynamic anchor makes recognition inapplicable.
            if !context.is_empty() {
                return Ok(None);
            }
            self.context_copy(&context)?;
            self.graph.charge(256)?;
            let visit = Visit {
                schema: node,
                instance,
                context: context.clone(),
            };
            if !seen.insert(visit.clone()) {
                return Ok(None);
            }
            let Some(shape) = self.cycle_shape(node)? else {
                return Ok(None);
            };
            match shape.operator {
                Operator::Reference => {
                    if shape.operand.view().as_str().is_none() {
                        return Ok(None);
                    }
                    let query = self.query(node, "$ref")?;
                    let target = match query.as_ref() {
                        Ok(target) => target,
                        Err(QueryFailure::ResourceUnavailable(uri)) => {
                            // Retain a real encountered cause, not a cycle verdict.
                            // Missing contents may contain assertions or producers.
                            let uri = self.copy_uri(Some(uri))?;
                            self.graph.charge(256)?;
                            let atom = Atom {
                                class: Class::External,
                                node,
                                keyword: "$ref".into(),
                                uri: None,
                                cause: None,
                            };
                            let mut detail = no_verdict(
                                NoVerdictReason::ResourceUnavailable,
                                "resource-unavailable",
                                "schema resolution encountered unavailable external content",
                            );
                            detail.location = Self::origin(self.plan, &atom, &mut self.graph)?;
                            *unavailable = uri;
                            return Err(detail);
                        }
                        Err(_) => return Ok(None),
                    };
                    node = target.node;
                }
                Operator::All | Operator::Any | Operator::One => {
                    let Some(mut children) = shape.operand.view().elements() else {
                        return Ok(None);
                    };
                    let Some(child) = children.next() else {
                        return Ok(None);
                    };
                    if children.next().is_some() {
                        return Ok(Some(visit));
                    }
                    node = self.child(node, child)?;
                }
                Operator::Not => return Ok(Some(visit)),
            }
        }
    }

    /// Fully close the reachable OR graph, then prove every canonical body can
    /// reach its root. Every node has a nonempty outgoing list, so this strongly
    /// connected graph is cyclic. Uniform false and uniform true cut completions
    /// preserve all its operators and yield opposing root outcomes.
    fn closed_or_cycle(
        &mut self,
        root: &Visit,
        unavailable: &mut Option<String>,
    ) -> Result<bool, NoVerdict> {
        self.context_copy(&root.context)?;
        self.graph.charge(768)?;
        let mut nodes = vec![root.clone()];
        let mut indices = HashMap::from([(root.clone(), 0usize)]);
        let mut reverse = vec![Vec::<usize>::new()];
        let mut cursor = 0;
        while cursor < nodes.len() {
            self.graph.tick()?;
            self.context_copy(&nodes[cursor].context)?;
            let visit = nodes[cursor].clone();
            let Some(shape) = self.cycle_shape(visit.schema)? else {
                return Ok(false);
            };
            if shape.operator != Operator::Any {
                return Ok(false);
            }
            let Some(children) = shape.operand.view().elements() else {
                return Ok(false);
            };
            let mut nonempty = false;
            for child in children {
                self.graph.tick()?;
                nonempty = true;
                let child = self.child(visit.schema, child)?;
                let Some(target) =
                    self.static_cycle_entry(child, visit.instance, &visit.context, unavailable)?
                else {
                    return Ok(false);
                };
                let index = if let Some(&index) = indices.get(&target) {
                    index
                } else {
                    self.context_copy(&target.context)?;
                    self.graph.charge(768)?;
                    let index = nodes.len();
                    nodes.push(target.clone());
                    indices.insert(target, index);
                    reverse.push(Vec::new());
                    index
                };
                self.graph.charge(64)?;
                reverse[index].push(cursor);
            }
            if !nonempty {
                return Ok(false);
            }
            cursor += 1;
        }
        self.graph
            .charge(nodes.len().saturating_mul(64).saturating_add(128))?;
        let mut reached = vec![false; nodes.len()];
        let mut pending = vec![0usize];
        reached[0] = true;
        while let Some(index) = pending.pop() {
            self.graph.tick()?;
            for &prior in &reverse[index] {
                self.graph.tick()?;
                if !reached[prior] {
                    reached[prior] = true;
                    pending.push(prior);
                }
            }
        }
        self.graph.work(reached.len())?;
        Ok(reached.into_iter().all(|yes| yes))
    }

    fn direct_cycle_certificate(
        &mut self,
        value: JsonRef<'_>,
        unavailable: &mut Option<String>,
    ) -> Result<Option<usize>, NoVerdict> {
        let empty = self.empty_context.clone();
        let Some(root) =
            self.static_cycle_entry(self.plan.entry, backend::ref_id(value), &empty, unavailable)?
        else {
            return Ok(None);
        };
        let Some(shape) = self.cycle_shape(root.schema)? else {
            return Ok(None);
        };
        let certified = if shape.operator == Operator::Not {
            let child = self.child(root.schema, shape.operand.view())?;
            self.static_cycle_entry(child, root.instance, &root.context, unavailable)?
                .as_ref()
                == Some(&root)
        } else if shape.operator == Operator::Any {
            self.closed_or_cycle(&root, unavailable)?
        } else {
            false
        };
        self.graph.tick()?;
        Ok(certified.then_some(root.schema))
    }

    pub(super) fn finish_root(
        &mut self,
        value: JsonRef<'_>,
        outcome: ValueOutcome,
        unavailable: &mut Option<String>,
    ) -> ValueOutcome {
        if !matches!(&outcome, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::UnsupportedCapability)
        {
            return outcome;
        }
        let mut encountered_unavailable = None;
        let proof = (|| {
            let Some(node) = self.direct_cycle_certificate(value, &mut encountered_unavailable)?
            else {
                return Ok(None);
            };
            self.graph.charge(256)?;
            let atom = Atom {
                class: Class::Undefined,
                node,
                keyword: "$cycle-certificate".into(),
                uri: None,
                cause: None,
            };
            let mut detail = UndefinedResult::new(
                "semantic-dependence",
                "the value's validity depends on a known undefined schema result",
            );
            detail.location = Self::origin(self.plan, &atom, &mut self.graph)?;
            self.graph.tick()?;
            Ok::<_, NoVerdict>(Some(ValueOutcome::Undefined { detail }))
        })();
        if let Err(detail) = self.graph.tick() {
            return ValueOutcome::NoVerdict { detail };
        }
        match proof {
            Ok(Some(certified)) => certified,
            Ok(None) => outcome,
            Err(detail) => {
                if detail.reason == NoVerdictReason::ResourceUnavailable {
                    *unavailable = encountered_unavailable;
                }
                ValueOutcome::NoVerdict { detail }
            }
        }
    }
}
