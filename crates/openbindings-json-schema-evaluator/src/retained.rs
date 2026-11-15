//! On-demand storage walk, compiled as a child of semantic to borrow its fields.
use super::*;
use openbindings_internal_json::backend::{hash_bytes, vec_bytes};

fn arc_bytes<T>(value: &Arc<T>) -> usize {
    size_of_val(&**value) + 2 * size_of::<usize>()
}

impl Plan {
    pub(crate) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + 2 * size_of::<usize>()
            + self.environment.retained_bytes()
            + vec_bytes(&self.custom)
            + vec_bytes(&self.resources)
            + hash_bytes(&self.formats)
            + hash_bytes(&self.queries)
            + hash_bytes(&self.patterns)
            + hash_bytes(&self.operands)
            + hash_bytes(&self.fields)
            + self.query_bytes()
            + self.pattern_bytes()
            + self.operand_bytes()
    }

    fn query_bytes(&self) -> usize {
        let mut seen = HashSet::new();
        self.queries
            .values()
            .filter(|query| seen.insert(Arc::as_ptr(query)))
            .map(|query| {
                arc_bytes(query)
                    + match query.as_ref() {
                        Ok(target) => target.dynamic_name.as_ref().map_or(0, String::capacity),
                        Err(
                            QueryFailure::ResourceUnavailable(uri)
                            | QueryFailure::UnsupportedDialect(Some(uri)),
                        ) => uri.capacity(),
                        _ => 0,
                    }
            })
            .sum()
    }

    fn pattern_bytes(&self) -> usize {
        let mut programs = HashSet::new();
        let mut matchers = HashSet::new();
        self.patterns
            .values()
            .filter(|program| programs.insert(Arc::as_ptr(program)))
            .map(|program| {
                arc_bytes(program)
                    + match program.as_ref() {
                        patterns::Program::Simple { matcher, .. }
                            if matchers.insert(Arc::as_ptr(matcher)) =>
                        {
                            arc_bytes(matcher) + matcher.heap_bytes()
                        }
                        // The engine exposes representation size, not allocator capacities.
                        patterns::Program::Vm { engine, .. } => {
                            engine.get_nfa().memory_usage() + 2 * size_of::<usize>()
                        }
                        _ => 0,
                    }
            })
            .sum()
    }

    fn operand_bytes(&self) -> usize {
        self.operands
            .values()
            .map(|prepared| {
                vec_bytes(&prepared.numbers)
                    + vec_bytes(&prepared.sizes)
                    + prepared
                        .numbers
                        .iter()
                        .map(|number| match &number.reading {
                            operands::Reading::Parsed(bound) => bound.retained_bytes(),
                            _ => 0,
                        })
                        .sum::<usize>()
            })
            .sum()
    }
}
