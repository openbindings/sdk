//! Call-local backing allocations with an explicit requested-size ledger.
//! Indexed entries support bounded lookup, append/pop and in-place value access.
use super::{Graph, NoVerdict, decision::limit};
use std::collections::BTreeMap;

/// Borrowed per-operation accounting; the storage never retains a control or
/// prescribes whether an exhaustion belongs to preparation or evaluation.
pub(super) trait Ledger {
    type Error;
    fn work(&mut self, units: usize) -> Result<(), Self::Error>;
    fn allocation(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn allocation_limit(&mut self) -> Self::Error;
}
impl Ledger for Graph<'_> {
    type Error = NoVerdict;
    fn work(&mut self, units: usize) -> Result<(), Self::Error> {
        Graph::work(self, units)
    }
    fn allocation(&mut self, bytes: usize) -> Result<(), Self::Error> {
        self.charge(bytes)
    }
    fn allocation_limit(&mut self) -> Self::Error {
        limit("semantic-analysis-memory-limit")
    }
}

pub(super) fn reserve<T, L: Ledger>(
    values: &mut Vec<T>,
    needed: usize,
    ledger: &mut L,
) -> Result<(), L::Error> {
    if needed <= values.capacity() {
        return Ok(());
    }
    let capacity = needed
        .max(
            values
                .capacity()
                .checked_mul(2)
                .ok_or_else(|| ledger.allocation_limit())?,
        )
        .max(4);
    let bytes = capacity
        .checked_mul(size_of::<T>())
        .ok_or_else(|| ledger.allocation_limit())?;
    ledger.work(1)?;
    // Charge the entire requested replacement allocation, including relocation
    // overlap with the previously charged backing store. Allocator bookkeeping
    // is outside this SDK requested-allocation policy.
    ledger.allocation(bytes)?;
    values
        .try_reserve_exact(capacity - values.len())
        .map_err(|_| ledger.allocation_limit())
}
struct Entry<K, V> {
    key: K,
    value: V,
    /// The newest earlier entry with an equal key, restored by a pop.
    shadowed: Option<usize>,
}
pub(super) struct Entries<K, V = ()> {
    entries: Vec<Entry<K, V>>,
    newest: BTreeMap<K, usize>,
    /// The most entries held at once; the index's storage is charged once for
    /// each, so a push after a pop pays nothing again.
    charged: usize,
}
impl<K, V> Default for Entries<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            newest: BTreeMap::new(),
            charged: 0,
        }
    }
}
impl<K: Ord + Clone, V> Entries<K, V> {
    // A search reads at most eleven keys in each node of a tree at most about
    // log6(n) tall, and never more keys than there are. `work` is the cost of
    // comparing two keys.
    fn search_work(&self, work: usize) -> usize {
        let len = self.entries.len();
        let height = (len + 1).ilog2() as usize / 2 + 1;
        height
            .saturating_mul(11)
            .min(len.max(1))
            .saturating_mul(work)
            .saturating_add(1)
    }
    pub(super) fn find<L: Ledger>(
        &self,
        key: &K,
        work: usize,
        ledger: &mut L,
    ) -> Result<Option<usize>, L::Error> {
        ledger.work(self.search_work(work))?;
        Ok(self.newest.get(key).copied())
    }
    pub(super) fn push<L: Ledger>(
        &mut self,
        key: K,
        value: V,
        work: usize,
        ledger: &mut L,
    ) -> Result<(), L::Error> {
        let needed = self
            .entries
            .len()
            .checked_add(1)
            .ok_or_else(|| ledger.allocation_limit())?;
        // Pay for this insertion and for its eventual pop.
        ledger.work(self.search_work(work).saturating_mul(2))?;
        reserve(&mut self.entries, needed, ledger)?;
        if needed > self.charged {
            // A half-full leaf holds five entries, and branches add under a
            // fifth more, so this covers a node's share per entry.
            ledger.allocation(3 * (size_of::<K>() + size_of::<usize>()) + 16)?;
            self.charged = needed;
        }
        let shadowed = self.newest.insert(key.clone(), self.entries.len());
        self.entries.push(Entry {
            key,
            value,
            shadowed,
        });
        Ok(())
    }
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(super) fn value(&self, index: usize) -> &V {
        &self.entries[index].value
    }
    pub(super) fn value_mut(&mut self, index: usize) -> &mut V {
        &mut self.entries[index].value
    }
    pub(super) fn pop(&mut self) -> Option<(K, V)> {
        let entry = self.entries.pop()?;
        match entry.shadowed {
            Some(prior) => *self.newest.get_mut(&entry.key).expect("a listed key") = prior,
            None => {
                self.newest.remove(&entry.key);
            }
        }
        Some((entry.key, entry.value))
    }
    pub(super) fn suffix(&self, start: usize) -> impl Iterator<Item = (&K, &V)> {
        self.entries[start..]
            .iter()
            .map(|entry| (&entry.key, &entry.value))
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.newest.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct Key(usize);
    #[test]
    fn ordered_lookup_pop_and_clear_preserve_exact_path_positions() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(1 << 20, None, 0, &control);
        let mut path = Entries::default();
        for key in 0..80 {
            path.push(Key(key), key, 1, &mut graph).unwrap();
            for existing in 0..=key {
                assert_eq!(
                    path.find(&Key(existing), 1, &mut graph).unwrap(),
                    Some(existing)
                );
            }
        }
        assert_eq!(path.find(&Key(90), 1, &mut graph).unwrap(), None);
        for key in (20..80).rev() {
            assert_eq!(path.pop().unwrap().1, key);
            assert_eq!(path.find(&Key(key), 1, &mut graph).unwrap(), None);
            assert_eq!(path.find(&Key(19), 1, &mut graph).unwrap(), Some(19));
        }
        assert_eq!(
            path.suffix(15).map(|(_, v)| *v).collect::<Vec<_>>(),
            (15..20).collect::<Vec<_>>()
        );
        path.push(Key(19), 100, 1, &mut graph).unwrap();
        assert_eq!(path.find(&Key(19), 1, &mut graph).unwrap(), Some(20));
        assert_eq!(path.pop().unwrap().1, 100);
        assert_eq!(path.find(&Key(19), 1, &mut graph).unwrap(), Some(19));
        path.clear();
        path.push(Key(19), 99, 1, &mut graph).unwrap();
        assert_eq!(path.find(&Key(19), 1, &mut graph).unwrap(), Some(0));
        assert_eq!(path.find(&Key(18), 1, &mut graph).unwrap(), None);
    }
    #[test]
    fn growth_refuses_before_allocation_and_search_work_is_bounded() {
        let control = AtomicBool::new(false);
        let mut values = Vec::<u64>::new();
        let bytes = 4 * size_of::<u64>();
        assert!(
            reserve(
                &mut values,
                1,
                &mut Graph::new(bytes - 1, None, 0, &control)
            )
            .is_err()
        );
        assert_eq!(values.capacity(), 0);
        reserve(&mut values, 1, &mut Graph::new(bytes, None, 0, &control)).unwrap();
        assert!(
            reserve(
                &mut values,
                usize::MAX,
                &mut Graph::new(usize::MAX, None, 0, &control)
            )
            .is_err()
        );
        let mut graph = Graph::new(1 << 20, None, 0, &control);
        let mut path = Entries::default();
        for key in 0..20 {
            path.push(Key(key), (), 1, &mut graph).unwrap();
        }
        assert!(
            path.find(&Key(99), 1, &mut Graph::new(0, Some(20), 0, &control))
                .is_err()
        );
        assert_eq!(
            path.find(&Key(99), 1, &mut Graph::new(0, Some(21), 0, &control))
                .unwrap(),
            None
        );
        assert!(
            path.find(
                &Key(99),
                1,
                &mut Graph::new(0, None, 0, &AtomicBool::new(true))
            )
            .is_err()
        );
    }
}
