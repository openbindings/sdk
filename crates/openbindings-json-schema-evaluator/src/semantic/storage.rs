//! Call-local backing allocations with an explicit requested-size ledger.
//! The ordered hash path supports append/pop and construction-local lookup.
use super::{Graph, NoVerdict, decision::limit};
use std::hash::{DefaultHasher, Hash, Hasher};

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
const NONE: usize = usize::MAX;
struct Entry<K, V> {
    key: K,
    value: V,
    hash: u64,
    prior: usize,
}
pub(super) struct Path<K, V = ()> {
    entries: Vec<Entry<K, V>>,
    buckets: Vec<usize>,
}
impl<K, V> Default for Path<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            buckets: Vec::new(),
        }
    }
}
impl<K: Hash + Eq, V> Path<K, V> {
    fn hash<L: Ledger>(key: &K, work: usize, ledger: &mut L) -> Result<u64, L::Error> {
        ledger.work(work)?;
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        Ok(hasher.finish())
    }
    pub(super) fn find<L: Ledger>(
        &self,
        key: &K,
        work: usize,
        ledger: &mut L,
    ) -> Result<Option<usize>, L::Error> {
        let hash = Self::hash(key, work, ledger)?;
        if self.buckets.is_empty() {
            return Ok(None);
        }
        let mut index = self.buckets[hash as usize & (self.buckets.len() - 1)];
        while index != NONE {
            let entry = &self.entries[index];
            ledger.work(work)?;
            if entry.hash == hash && entry.key == *key {
                return Ok(Some(index));
            }
            index = entry.prior;
        }
        Ok(None)
    }
    pub(super) fn push<L: Ledger>(
        &mut self,
        key: K,
        value: V,
        work: usize,
        ledger: &mut L,
    ) -> Result<(), L::Error> {
        let hash = Self::hash(&key, work, ledger)?;
        let needed = self
            .entries
            .len()
            .checked_add(1)
            .ok_or_else(|| ledger.allocation_limit())?;
        reserve(&mut self.entries, needed, ledger)?;
        if needed
            .checked_mul(2)
            .ok_or_else(|| ledger.allocation_limit())?
            > self.buckets.len()
        {
            let count = self
                .buckets
                .len()
                .checked_mul(2)
                .ok_or_else(|| ledger.allocation_limit())?
                .max(8);
            reserve(&mut self.buckets, count, ledger)?;
            let work = count
                .checked_add(self.entries.len())
                .ok_or_else(|| ledger.allocation_limit())?;
            ledger.work(work)?;
            self.buckets.resize(count, NONE);
            self.buckets.fill(NONE);
            for (index, entry) in self.entries.iter_mut().enumerate() {
                let bucket = entry.hash as usize & (count - 1);
                entry.prior = self.buckets[bucket];
                self.buckets[bucket] = index;
            }
        }
        let bucket = hash as usize & (self.buckets.len() - 1);
        let prior = self.buckets[bucket];
        self.buckets[bucket] = self.entries.len();
        self.entries.push(Entry {
            key,
            value,
            hash,
            prior,
        });
        Ok(())
    }
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(super) fn value(&self, index: usize) -> &V {
        &self.entries[index].value
    }
    pub(super) fn pop(&mut self) -> Option<(K, V)> {
        let entry = self.entries.pop()?;
        let bucket = entry.hash as usize & (self.buckets.len() - 1);
        assert_eq!(
            self.buckets[bucket],
            self.entries.len(),
            "ordered path bucket head"
        );
        self.buckets[bucket] = entry.prior;
        Some((entry.key, entry.value))
    }
    pub(super) fn suffix(&self, start: usize) -> impl Iterator<Item = (&K, &V)> {
        self.entries[start..]
            .iter()
            .map(|entry| (&entry.key, &entry.value))
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.buckets.fill(NONE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[derive(PartialEq, Eq)]
    struct Collision(usize);
    impl Hash for Collision {
        fn hash<H: Hasher>(&self, state: &mut H) {
            0usize.hash(state);
        }
    }
    #[test]
    fn collisions_rehash_pop_and_clear_preserve_exact_path_positions() {
        let control = AtomicBool::new(false);
        let mut graph = Graph::new(1 << 20, None, 0, &control);
        let mut path = Path::default();
        for key in 0..80 {
            path.push(Collision(key), key, 1, &mut graph).unwrap();
            for existing in 0..=key {
                assert_eq!(
                    path.find(&Collision(existing), 1, &mut graph).unwrap(),
                    Some(existing)
                );
            }
        }
        assert_eq!(path.find(&Collision(90), 1, &mut graph).unwrap(), None);
        for key in (20..80).rev() {
            assert_eq!(path.pop().unwrap().1, key);
            assert_eq!(path.find(&Collision(key), 1, &mut graph).unwrap(), None);
            assert_eq!(path.find(&Collision(19), 1, &mut graph).unwrap(), Some(19));
        }
        assert_eq!(
            path.suffix(15).map(|(_, v)| *v).collect::<Vec<_>>(),
            (15..20).collect::<Vec<_>>()
        );
        path.clear();
        path.push(Collision(19), 99, 1, &mut graph).unwrap();
        assert_eq!(path.find(&Collision(19), 1, &mut graph).unwrap(), Some(0));
        assert_eq!(path.find(&Collision(18), 1, &mut graph).unwrap(), None);
    }
    #[test]
    fn growth_refuses_before_allocation_and_collision_work_is_bounded() {
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
        let mut path = Path::default();
        for key in 0..20 {
            path.push(Collision(key), (), 1, &mut graph).unwrap();
        }
        assert!(
            path.find(&Collision(99), 1, &mut Graph::new(0, Some(10), 0, &control))
                .is_err()
        );
        assert!(
            path.find(
                &Collision(99),
                1,
                &mut Graph::new(0, None, 0, &AtomicBool::new(true))
            )
            .is_err()
        );
    }
}
