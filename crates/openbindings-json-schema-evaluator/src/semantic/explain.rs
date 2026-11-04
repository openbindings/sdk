//! Optional evidence for one concrete failure in the original interpreter.
use super::*;

pub(super) struct WitnessSeed {
    schema: usize,
    keyword: Option<&'static str>,
    instance: usize,
}

impl State {
    pub(super) fn inherit_failure(&mut self, other: &Self, applies: Formula) {
        if self.valid == FALSE && other.valid == FALSE && applies == TRUE && self.witness.is_none()
        {
            self.witness = other.witness.clone();
        }
    }
}

impl Run<'_> {
    pub(super) fn record_failure(
        &mut self,
        state: &mut State,
        schema: usize,
        value: JsonRef<'_>,
        keyword: Option<&'static str>,
    ) -> Result<(), NoVerdict> {
        if self.record_witnesses && state.valid == FALSE && state.witness.is_none() {
            self.graph.tick()?;
            // Arc's two word-sized counters precede this word-aligned payload.
            self.graph
                .charge(size_of::<WitnessSeed>() + 2 * size_of::<usize>())?;
            state.witness = Some(Arc::new(WitnessSeed {
                schema,
                keyword,
                instance: backend::ref_id(value),
            }));
        }
        Ok(())
    }

    pub(super) fn materialize(
        &mut self,
        seed: &WitnessSeed,
        root: &JsonValue,
    ) -> Result<Option<ValueProblem>, NoVerdict> {
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
            size_of::<ValueProblem>(),
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
        Ok(Some(problem))
    }
}

#[cfg(test)]
mod tests;
