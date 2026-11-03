//! Contained-resource admission, using OBI-13's exact compared identifiers.
//! The compact identifier table is shared with duplicate checking so deeply
//! nested relative IDs never require retaining every expanded URI string.
use super::*;

impl SchemaIndex {
    pub(super) fn check_identified_references<'a>(
        &mut self,
        source: &'a JsonValue,
        ids: &mut Identifiers<'a>,
        declarations: &[u32],
    ) {
        // All declaration handles precede query handles. This avoids retaining
        // another per-schema base/usage table for documents with many identifiers.
        let mut references = Vec::new();
        for node in &self.nodes {
            let base = node
                .resource
                .checked_sub(1)
                .and_then(|index| declarations.binary_search(&index).ok())
                .map(|h| h as u32);
            if node.resource != 0 && base.is_none() {
                continue; // OBI-12 explicitly excludes an unusable declared base.
            }
            for keyword in ["$ref", "$dynamicRef"] {
                if let Some(reference) = node_of(Some(source), node.id).get(keyword)
                    && let Some(text) = reference.as_str()
                    && crate::uri::valid(text)
                    && let Some(handle) = ids.compared(base, text.split('#').next().unwrap())
                {
                    debug_assert_eq!(handle as usize, declarations.len() + references.len());
                    references.push(ref_id(reference) as u32);
                }
            }
        }
        let equal = ids.duplicates();
        for group in equal.chunk_by(|a, b| a.0 == b.0) {
            let declared: Vec<u32> = group
                .iter()
                .filter_map(|&(_, handle)| declarations.get(handle as usize).copied())
                .collect();
            if declared.len() > 1 {
                self.identifiers
                    .extend(declared.iter().map(|&index| (group[0].0, index)));
            }
            if declared.is_empty() {
                continue;
            }
            for &(_, handle) in group {
                if let Some(&token) = (handle as usize)
                    .checked_sub(declarations.len())
                    .and_then(|index| references.get(index))
                {
                    let reference = node_of(self.source.as_ref(), token).as_str().unwrap();
                    // Distinct occurrences declaring one URI cannot select a
                    // unique initial target, even if their bytes are equal.
                    // Do not invent a first/last-writer winner or search every
                    // duplicated resource for each reference.
                    if declared.len() != 1
                        || !self.fragment_identifies_schema(declared[0] as usize, reference)
                    {
                        self.missing_targets.push(token);
                    }
                }
            }
        }
    }

    fn fragment_identifies_schema(&self, root: usize, reference: &str) -> bool {
        let fragment = crate::uri::fragment(reference).unwrap_or("");
        let Some(fragment) = crate::uri::decode_fragment(fragment) else {
            return false;
        };
        if fragment.is_empty() {
            return true;
        }
        if fragment.starts_with('/') {
            return self
                .value(&self.nodes[root])
                .at(&fragment)
                .is_some_and(|value| self.position(ref_id(value)).is_some());
        }
        self.named(root as u32 + 1, &fragment).len() == 1
    }

    /// Original reference tokens failing OBI-12's contained-resource clause.
    /// Unavailable external resources and malformed references inside identified
    /// resources are not document-admission failures under this rule.
    pub fn missing_contained_targets(&self) -> impl Iterator<Item = JsonRef<'_>> {
        self.missing_targets
            .iter()
            .map(|&id| node_of(self.source.as_ref(), id))
    }
}
