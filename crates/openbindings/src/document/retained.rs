//! On-demand estimates. None of these walks run during parsing or validation.
use super::*;
use backend::{btree_bytes, pointer_retained_bytes, vec_bytes};

fn namespace(entries: &index::Namespace) -> usize {
    btree_bytes(entries)
        + entries
            .keys()
            .map(|key| key.len() + 2 * size_of::<usize>())
            .sum::<usize>()
}
fn cached_namespace(cache: &NamespaceCache) -> usize {
    cache.get().and_then(Option::as_ref).map_or(0, namespace)
}
fn origin(origin: &SchemaOrigin) -> usize {
    origin.source_uri.as_ref().map_or(0, String::capacity) + pointer_retained_bytes(&origin.pointer)
}
fn strings(values: &Vec<String>) -> usize {
    vec_bytes(values) + values.iter().map(String::capacity).sum::<usize>()
}
fn details(details: &ValueProblemDetails) -> usize {
    match details {
        ValueProblemDetails::Type { expected } => strings(expected),
        ValueProblemDetails::Enum { choices } => strings(choices),
        ValueProblemDetails::Required { member } => member.capacity(),
        ValueProblemDetails::NumericBound { bound } | ValueProblemDetails::SizeBound { bound } => {
            bound.capacity()
        }
    }
}
fn report(report: &ValidationReport) -> usize {
    size_of::<ValidationReport>()
        + 2 * size_of::<usize>()
        + btree_bytes(&report.evidence)
        + vec_bytes(&report.findings)
        + report
            .findings
            .iter()
            .map(|finding| {
                finding.message.capacity()
                    + finding
                        .location
                        .as_ref()
                        .and_then(|l| l.pointer.as_ref())
                        .map_or(0, pointer_retained_bytes)
                    + finding.schema.as_ref().map_or(0, origin)
                    + finding.details.as_ref().map_or(0, details)
            })
            .sum::<usize>()
}
impl Document {
    /// Estimate heap bytes retained by this snapshot's shared storage now.
    ///
    /// Includes original source text, parsed nodes and buffers, and currently
    /// built document indexes, conformance reports and the schema resolution
    /// cached for preparation without supplied resources. A resolution built
    /// with supplied resources is held by that [`ExternalSchemas`](crate::ExternalSchemas) set and is
    /// not counted here.
    /// Uses existing capacities; it does not parse, validate or initialize caches.
    /// Clones and proofs share storage: charge a snapshot once, reaching it from
    /// a proof through [`ConformantDocument::document`]. Separately owned external
    /// resources, evaluator validators and process-wide packaged schemas are not
    /// part of the snapshot. Excludes allocator overhead and Wasm memory growth.
    ///
    /// A later lookup can add namespace entries and aliases proportional to the
    /// names indexed, schema nodes and identities proportional to the schema
    /// inventory, or source-position checkpoints (two machine words per 4096
    /// source bytes, with a geometrically reserved buffer capped at
    /// `source_bytes / 4096` checkpoints). Thus a location lookup adds at most
    /// `2 * size_of::<usize>() * (source_bytes / 4096)` checkpoint bytes.
    /// Namespace caches copy their indexed names/aliases and reserve tree nodes;
    /// schema caches retain their inventory entries and identity text. Their
    /// growth depends on this document, so compare calls before and after lookup.
    /// Validation can retain
    /// its bounded report. Call again after those operations to include growth.
    /// Tree and hash allocation sizes are estimates because their capacities or
    /// node occupancy are not exposed; this is not a process-memory measurement.
    pub fn retained_bytes(&self) -> usize {
        let inner = &self.inner;
        let mut bytes = size_of::<DocumentInner>()
            + 2 * size_of::<usize>()
            + backend::retained_bytes(&inner.value)
            + vec_bytes(
                &inner
                    .spaces_in
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            );
        if let Some(index) = inner.schemas.get() {
            bytes += index.retained_bytes();
        }
        if let Some(space) = inner.resolution.get() {
            bytes += space.retained_bytes();
        }
        if let Some(validation) = inner.validation.get() {
            bytes += match validation {
                Ok(value) => report(value),
                Err(refusal) => refusal.declared.capacity(),
            };
        }
        if let Some(operations) = inner.operations.get() {
            bytes += btree_bytes(&operations.entries) + btree_bytes(&operations.names);
            for (key, entry) in &operations.entries {
                bytes += key.len() + 2 * size_of::<usize>() + cached_namespace(&entry.examples);
            }
            for key in operations.names.keys() {
                if !operations.entries.contains_key(key) {
                    bytes += key.len() + 2 * size_of::<usize>();
                }
            }
        }
        if let Some(bindings) = inner.bindings.get() {
            bytes +=
                bindings.entries.as_ref().map_or(0, namespace) + btree_bytes(&bindings.operations);
            for (key, values) in &bindings.operations {
                bytes += key.len() + 2 * size_of::<usize>() + vec_bytes(values);
            }
        }
        bytes + cached_namespace(&inner.sources) + cached_namespace(&inner.dependencies)
    }
}
