//! Static resolution shared only within one immutable original environment.
//! This lookup borrows source text; only query-result Arcs enter the plan.
use super::*;
use crate::semantic::storage::{Ledger, Path};
use openbindings_internal_schema::Control;

const MAX_KEYS: usize = 1024;

#[derive(Hash, PartialEq, Eq)]
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
        limit("preparation-allocation-bytes")
    }
}

pub(super) struct Queries<'s> {
    space: &'s SchemaSpace,
    entries: Path<Key<'s>, ReferenceQuery>,
}
impl<'s> Queries<'s> {
    pub(super) fn new(space: &'s SchemaSpace) -> Self {
        Self {
            space,
            entries: Path::default(),
        }
    }
    pub(super) fn query(
        &mut self,
        node: usize,
        reference: &'s str,
        control: &dyn Control,
    ) -> Result<ReferenceQuery, PreparationFailure> {
        let mut ledger = Preparation(control);
        let space = self.space;
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
        // Misses retain the full URI parsing/normalization reservation. The
        // fixed envelope includes the query-result Arc and resolver headers.
        let base = space
            .resource(original.resource)
            .base
            .as_ref()
            .map_or(0, String::len);
        let bytes = base
            .checked_add(reference.len())
            .and_then(|n| n.checked_mul(16))
            .and_then(|n| n.checked_add(512))
            .ok_or_else(|| ledger.allocation_limit())?;
        control.charge_work(bytes - 511)?;
        control.charge_allocation(bytes)?;
        let query = Arc::new(space.query(node, reference));
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
