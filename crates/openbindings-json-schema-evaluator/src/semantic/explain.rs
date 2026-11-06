//! Bounded evidence from the original interpreter, collected only after failure.
use super::*;

// Bound retained assertions independently of the caller's work/storage allowance.
pub(super) const MAX_EXPLANATION_PROBLEMS: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct WitnessSeed {
    schema: usize,
    keyword: Option<&'static str>,
    instance: usize,
    // Original required-array member, including dependencies.
    member: Option<usize>,
}

#[derive(Clone)]
pub(super) struct Witnesses {
    seeds: Vec<WitnessSeed>,
    complete: bool,
}

impl Witnesses {
    pub(super) fn valid_for(&self, valid: Formula) -> bool {
        self.seeds.len() <= MAX_EXPLANATION_PROBLEMS
            && (self.seeds.is_empty() || valid == FALSE)
            && (!self.seeds.is_empty() || !self.complete)
    }
}

fn witnesses<'a>(
    state: &'a mut State,
    graph: &mut Graph<'_>,
) -> Result<&'a mut Witnesses, NoVerdict> {
    let shared = state
        .witness
        .as_ref()
        .is_some_and(|v| Arc::strong_count(v) > 1);
    if state.witness.is_none() || shared {
        let count = state.witness.as_ref().map_or(0, |v| v.seeds.len());
        graph.work(count + 1)?;
        graph.charge(
            size_of::<Witnesses>() + 2 * size_of::<usize>() + count * size_of::<WitnessSeed>(),
        )?;
    }
    let evidence = state.witness.get_or_insert_with(|| {
        Arc::new(Witnesses {
            seeds: Vec::new(),
            complete: true,
        })
    });
    Ok(Arc::make_mut(evidence))
}

impl Run<'_> {
    pub(super) fn uncertain_failure(
        &mut self,
        state: &mut State,
        formula: Formula,
    ) -> Result<(), NoVerdict> {
        if self.record_witnesses && formula > TRUE {
            witnesses(state, &mut self.graph)?.complete = false;
        }
        Ok(())
    }

    pub(super) fn inherit_failure(
        &mut self,
        state: &mut State,
        other: &State,
        applies: Formula,
    ) -> Result<(), NoVerdict> {
        if !self.record_witnesses || applies == FALSE || other.valid == TRUE {
            return Ok(());
        }
        if applies != TRUE || other.valid != FALSE {
            witnesses(state, &mut self.graph)?.complete = false;
            return Ok(());
        }
        let Some(evidence) = &other.witness else {
            witnesses(state, &mut self.graph)?.complete = false;
            return Ok(());
        };
        if state.witness.is_none() {
            state.witness = Some(evidence.clone());
            return Ok(());
        }
        for seed in &evidence.seeds {
            self.retain_failure(state, *seed)?;
        }
        if !evidence.complete {
            witnesses(state, &mut self.graph)?.complete = false;
        }
        Ok(())
    }

    fn retain_failure(&mut self, state: &mut State, seed: WitnessSeed) -> Result<(), NoVerdict> {
        let evidence = witnesses(state, &mut self.graph)?;
        self.graph.work(evidence.seeds.len() + 1)?;
        // Repeated references to the same assertion and occurrence are one problem.
        if evidence.seeds.contains(&seed) {
            return Ok(());
        }
        if evidence.seeds.len() == MAX_EXPLANATION_PROBLEMS {
            evidence.complete = false;
            return Ok(());
        }
        let needed = evidence.seeds.len() + 1;
        storage::reserve(&mut evidence.seeds, needed, &mut self.graph)?;
        evidence.seeds.push(seed);
        Ok(())
    }

    pub(super) fn record_failure(
        &mut self,
        state: &mut State,
        schema: usize,
        value: JsonRef<'_>,
        keyword: Option<&'static str>,
    ) -> Result<(), NoVerdict> {
        if !self.record_witnesses {
            return Ok(());
        }
        self.retain_failure(
            state,
            WitnessSeed {
                schema,
                keyword,
                instance: backend::ref_id(value),
                member: None,
            },
        )
    }

    pub(super) fn explain_primitives(
        &mut self,
        state: &mut State,
        node: usize,
        schema: &Interpreted<'_>,
        value: JsonRef<'_>,
    ) -> Result<Option<&'static str>, NoVerdict> {
        // The primitive vocabulary is finite. The ordinary visitor keeps only
        // its first failure; this visitor collects without an unbounded buffer.
        let mut failed = [None; 32];
        let mut count = 0;
        let mut complete = true;
        self.visit_primitives(node, schema, value, |valid, keyword| {
            if !valid && !failed.contains(&Some(keyword)) {
                if count < failed.len() {
                    failed[count] = Some(keyword);
                    count += 1;
                } else {
                    complete = false;
                }
            }
        })?;
        for keyword in failed.iter().flatten().copied() {
            if matches!(keyword, "required" | "dependentRequired" | "dependencies") {
                self.explain_required(state, node, schema, value, keyword)?;
            } else {
                self.record_failure(state, node, value, Some(keyword))?;
            }
        }
        if !complete {
            witnesses(state, &mut self.graph)?.complete = false;
        }
        Ok(failed[0])
    }

    fn explain_required(
        &mut self,
        state: &mut State,
        node: usize,
        schema: &Interpreted<'_>,
        value: JsonRef<'_>,
        keyword: &'static str,
    ) -> Result<(), NoVerdict> {
        let operand = schema.get(keyword).expect("failed required operand");
        if keyword == "required" {
            return self.missing_members(state, node, value, keyword, operand);
        }
        for dependency in operand.members().expect("validated dependencies") {
            self.graph.tick()?;
            if dependency.value.kind() == JsonKind::Array
                && self.named(value, dependency.name)?.is_some()
            {
                self.missing_members(state, node, value, keyword, dependency.value)?;
            }
        }
        Ok(())
    }

    fn missing_members(
        &mut self,
        state: &mut State,
        node: usize,
        value: JsonRef<'_>,
        keyword: &'static str,
        operand: JsonRef<'_>,
    ) -> Result<(), NoVerdict> {
        for member in operand.elements().expect("validated required array") {
            self.graph.tick()?;
            if self.named(value, member)?.is_none() {
                self.retain_failure(
                    state,
                    WitnessSeed {
                        schema: node,
                        keyword: Some(keyword),
                        instance: backend::ref_id(value),
                        member: Some(backend::ref_id(member)),
                    },
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn materialize_all(
        &mut self,
        witnesses: &Witnesses,
        root: &JsonValue,
    ) -> (Vec<ValueProblem>, bool) {
        let mut problems = Vec::new();
        let mut sort_width = 0usize;
        let mut complete = witnesses.complete && !witnesses.seeds.is_empty();
        for seed in &witnesses.seeds {
            let next = self.materialize(seed, root).and_then(|problem| {
                storage::reserve(&mut problems, witnesses.seeds.len(), &mut self.graph)?;
                Ok(problem)
            });
            match next {
                Ok(Some((problem, width))) => {
                    sort_width = sort_width.max(width);
                    problems.push(problem);
                }
                Ok(None) => complete = false,
                Err(_) => {
                    complete = false;
                    break;
                }
            }
        }
        // Charge a conservative comparison bound before sorting. In-place sort
        // needs no scratch allocation and never compares rejected value text.
        if problems.len() < 2 {
            return (problems, complete);
        }
        if self
            .graph
            .work(
                sort_width
                    .saturating_mul(problems.len().saturating_mul(problems.len()))
                    .saturating_mul(8)
                    .saturating_add(1),
            )
            .is_err()
        {
            return (Vec::new(), false);
        }
        problems.sort_unstable_by(|a, b| {
            a.instance
                .utf16_units()
                .cmp(b.instance.utf16_units())
                .then_with(|| a.schema.source_uri.cmp(&b.schema.source_uri))
                .then_with(|| {
                    a.schema
                        .pointer
                        .utf16_units()
                        .cmp(b.schema.pointer.utf16_units())
                })
        });
        (problems, complete)
    }

    pub(super) fn materialize(
        &mut self,
        seed: &WitnessSeed,
        root: &JsonValue,
    ) -> Result<Option<(ValueProblem, usize)>, NoVerdict> {
        let Some(instance) = backend::node(root, seed.instance) else {
            return Ok(None);
        };
        let node = self.plan.environment.space().node(seed.schema);
        let source = self.plan.environment.space().source(node.source);
        // The exact source extents bound all ancestor-name scans and final
        // writing, including UTF-16 decoding and keyword lookup. Include the
        // original source URI's copy; no temporary ancestry is allocated.
        let work = root
            .text()
            .len()
            .checked_add(source.value.text().len())
            .and_then(|size| size.checked_add(source.uri.as_ref().map_or(0, String::len)))
            .and_then(|size| size.checked_mul(8))
            .and_then(|size| size.checked_add(1))
            .ok_or_else(|| decision::limit("evaluation-work-limit"))?;
        self.graph.work(work)?;
        let schema = if let Some(keyword) = seed.keyword {
            let Some(value) = node.value.view().get(keyword) else {
                return Ok(None);
            };
            value
        } else {
            node.value.view()
        };
        let available = self.graph.remaining().bytes;
        let Some(instance_bytes) = backend::relative_pointer_size(instance, root.view(), available)
        else {
            return Ok(None);
        };
        let Some(schema_bytes) =
            backend::relative_pointer_size(schema, source.value.view(), available)
        else {
            return Ok(None);
        };
        let code = seed.keyword.unwrap_or("falseSchema");
        let message = "value fails this schema assertion";
        let bytes = [
            instance_bytes,
            schema_bytes,
            source.uri.as_ref().map_or(0, String::len),
            code.len(),
            message.len(),
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| decision::limit("semantic-analysis-memory-limit"))?;
        self.graph.tick()?;
        self.graph.charge(bytes)?;
        let problem = ValueProblem::new(
            backend::relative_pointer(instance, root.view()),
            SchemaOrigin::new(
                source.uri.clone(),
                backend::relative_pointer(schema, source.value.view()),
            ),
            code,
            message,
        );
        self.graph.tick()?;
        Ok(Some((problem, bytes)))
    }
}

#[cfg(test)]
mod tests;
