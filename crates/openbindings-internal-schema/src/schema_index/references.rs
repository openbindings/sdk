//! Contained-resource admission, using OBI-13's exact compared identifiers.
//! The compact identifier table is shared with duplicate checking so deeply
//! nested relative IDs never require retaining every expanded URI string.
use super::*;

pub(super) enum IdentifierUse {
    Declaration(u32),
    Reference(u32),
}

impl SchemaIndex {
    pub(super) fn check_identified_references(
        &mut self,
        ids: &Identifiers<'_>,
        uses: &[IdentifierUse],
    ) {
        let equal = ids.duplicates();
        for group in equal.chunk_by(|a, b| a.0 == b.0) {
            let declarations: Vec<u32> = group
                .iter()
                .filter_map(|&(_, handle)| match uses[handle as usize] {
                    IdentifierUse::Declaration(index) => Some(index),
                    IdentifierUse::Reference(_) => None,
                })
                .collect();
            if declarations.len() > 1 {
                self.identifiers
                    .extend(declarations.iter().map(|&index| (group[0].0, index)));
            }
            if declarations.is_empty() {
                continue;
            }
            for &(_, handle) in group {
                if let IdentifierUse::Reference(token) = uses[handle as usize] {
                    let reference = node_of(self.source.as_ref(), token).as_str().unwrap();
                    // Distinct occurrences declaring one URI cannot select a
                    // unique initial target, even if their bytes are equal.
                    // Do not invent a first/last-writer winner or search every
                    // duplicated resource for each reference.
                    if declarations.len() != 1
                        || !self.fragment_identifies_schema(declarations[0] as usize, reference)
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
