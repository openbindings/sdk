//! On-demand accounting for the environment's shared source and cache owners.
use super::*;
use openbindings_internal_json::backend::{self, vec_bytes};
use std::collections::HashSet;

#[derive(Default)]
struct Storage {
    arenas: HashSet<usize>,
    spaces: HashSet<*const SchemaSpace>,
    indexes: HashSet<*const openbindings_internal_schema::schema_index::SchemaIndex>,
}
impl Storage {
    fn exclude_document(&mut self, space: &SchemaSpace) {
        self.indexes.insert(space.document.schema_index());
        self.arenas
            .insert(backend::occurrence(space.document.value().view()).0);
    }

    fn source(&mut self, value: &JsonValue) -> usize {
        if self.arenas.insert(backend::occurrence(value.view()).0) {
            backend::retained_bytes(value)
        } else {
            0
        }
    }
    fn space(&mut self, space: &Arc<SchemaSpace>) -> usize {
        if self.spaces.insert(Arc::as_ptr(space)) {
            space.retained_bytes()
                + if self.indexes.insert(space.document.schema_index()) {
                    space.document.schema_index().retained_bytes()
                } else {
                    0
                }
                + space
                    .retained_sources()
                    .map(|value| self.source(value))
                    .sum::<usize>()
        } else {
            0
        }
    }
}
impl ExternalSchemas {
    pub(crate) fn environment_retained_bytes(
        &self,
        document: Option<&ConformantDocument>,
        space: &Arc<SchemaSpace>,
    ) -> usize {
        let mut storage = Storage::default();
        // A contained document is counted below. Supplied roots instead use
        // the resolver's process-wide empty document, which this owner excludes.
        storage.exclude_document(space);
        let mut bytes = 0;
        if let Some(document) = document {
            let document = document.document();
            bytes += document.retained_bytes();
            if let Some(cached) = document.retained_resolution() {
                storage.spaces.insert(Arc::as_ptr(cached));
            }
        }
        let inner = &self.inner;
        bytes += size_of::<ExternalSchemasInner>()
            + 2 * size_of::<usize>()
            + vec_bytes(&inner.resources)
            + inner.input_policy.retained_bytes();
        for resource in &inner.resources {
            bytes += resource.uri.capacity() + storage.source(&resource.document);
        }
        let spaces = inner.spaces.lock().unwrap_or_else(PoisonError::into_inner);
        bytes += vec_bytes(&spaces);
        for (_, cached) in spaces.iter() {
            bytes += storage.space(cached);
        }
        if let Some(cached) = inner.supplied_space.get() {
            storage.exclude_document(cached);
            bytes += storage.space(cached);
        }
        bytes + storage.space(space)
    }
}
