//! Static resolution shared only within one immutable original environment.
//! This lookup borrows source text; only query-result Arcs enter the plan.
use super::*;
use crate::semantic::storage::{Entries, Ledger};
use openbindings_internal_schema::Control;

const MAX_KEYS: usize = 1024;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key<'s> {
    resource: usize,
    profile_known: bool,
    reference: &'s str,
}

struct Preparation<'a>(&'a dyn Control);
impl Ledger for Preparation<'_> {
    type Error = PreparationFailure;
    fn work(&mut self, units: usize) -> Result<(), Self::Error> {
        self.0.charge_work(units).map_err(Into::into)
    }
    fn allocation(&mut self, bytes: usize) -> Result<(), Self::Error> {
        self.0.charge_allocation(bytes).map_err(Into::into)
    }
    fn allocation_limit(&mut self) -> Self::Error {
        self.0.allocation_limit_reached();
        limit("preparation-sdk-reservation-bytes")
    }
}

pub(super) struct Queries<'s> {
    resolver: openbindings_internal_schema::ReferenceResolver<'s>,
    entries: Entries<Key<'s>, ReferenceQuery>,
}
impl<'s> Queries<'s> {
    pub(super) fn new(space: &'s SchemaSpace) -> Self {
        Self {
            resolver: space.reference_resolver(),
            entries: Entries::default(),
        }
    }
    pub(super) fn query(
        &mut self,
        node: usize,
        reference: &'s str,
        control: &dyn Control,
    ) -> Result<ReferenceQuery, PreparationFailure> {
        let mut ledger = Preparation(control);
        let space = self.resolver.space();
        let original = space.node(node);
        // query() reads these holder-dependent facts, then the immutable
        // resource/namespace/fragment inventories. Do not normalize the key.
        let key = Key {
            resource: original.resource,
            profile_known: original.profile.is_some(),
            reference,
        };
        let work = reference
            .len()
            .checked_add(size_of::<usize>() + 2)
            .ok_or_else(|| ledger.allocation_limit())?;
        if let Some(index) = self.entries.find(&key, work, &mut ledger)? {
            return Ok(self.entries.value(index).clone());
        }
        // The original resolver reserves its compact input, then reserves an
        // expanded name only if the query result will own that missing URI.
        let query = Arc::new(self.resolver.query_controlled(node, reference, control)?);
        // Saturating this optimization does not refuse otherwise admissible
        // schemas. Existing hits remain useful; new keys use the ordinary path.
        if self.entries.len() < MAX_KEYS {
            self.entries.push(key, query.clone(), work, &mut ledger)?;
        }
        Ok(query)
    }
}

#[cfg(test)]
mod tests;
