//! Immutable environment inventory with request-local additions and trie splits.
use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    ops::{Index, IndexMut, RangeFrom},
};

#[derive(Debug)]
pub(super) struct Vector<'a, T> {
    base: &'a [T],
    added: Vec<T>,
    changed: HashMap<usize, T>,
}
impl<T> From<Vec<T>> for Vector<'_, T> {
    fn from(added: Vec<T>) -> Self {
        Self {
            base: &[],
            added,
            changed: HashMap::new(),
        }
    }
}
impl<T> Vector<'_, T> {
    pub fn retained_bytes(&self) -> usize {
        openbindings_internal_json::backend::vec_bytes(&self.added)
            + openbindings_internal_json::backend::hash_bytes(&self.changed)
    }
    pub fn new() -> Self {
        Vec::new().into()
    }
    pub fn overlay(&self) -> Vector<'_, T> {
        assert!(self.base.is_empty() && self.changed.is_empty());
        Vector {
            base: &self.added,
            added: Vec::new(),
            changed: HashMap::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.base.len() + self.added.len()
    }
    pub fn push(&mut self, value: T) {
        self.added.push(value);
    }
}
impl<T> Index<usize> for Vector<'_, T> {
    type Output = T;
    fn index(&self, at: usize) -> &T {
        if at < self.base.len() {
            self.changed.get(&at).unwrap_or(&self.base[at])
        } else {
            &self.added[at - self.base.len()]
        }
    }
}
impl<T: Clone> IndexMut<usize> for Vector<'_, T> {
    fn index_mut(&mut self, at: usize) -> &mut T {
        if at < self.base.len() {
            self.changed
                .entry(at)
                .or_insert_with(|| self.base[at].clone())
        } else {
            &mut self.added[at - self.base.len()]
        }
    }
}
impl<T> Index<RangeFrom<usize>> for Vector<'_, T> {
    type Output = [T];
    fn index(&self, range: RangeFrom<usize>) -> &[T] {
        &self.added[range
            .start
            .checked_sub(self.base.len())
            .expect("new entries")..]
    }
}
impl<T> IndexMut<RangeFrom<usize>> for Vector<'_, T> {
    fn index_mut(&mut self, range: RangeFrom<usize>) -> &mut [T] {
        &mut self.added[range
            .start
            .checked_sub(self.base.len())
            .expect("new entries")..]
    }
}

#[derive(Debug)]
pub(super) struct Table<'a, K, V> {
    base: Option<&'a BTreeMap<K, V>>,
    added: BTreeMap<K, V>,
}
impl<K: Ord, V> Table<'_, K, V> {
    pub fn retained_bytes(&self) -> usize {
        openbindings_internal_json::backend::btree_bytes(&self.added)
    }
    pub fn owned_keys(&self) -> impl Iterator<Item = &K> {
        self.added.keys()
    }
    pub fn new() -> Self {
        Self {
            base: None,
            added: BTreeMap::new(),
        }
    }
    pub fn overlay(&self) -> Table<'_, K, V> {
        assert!(self.base.is_none());
        Table {
            base: Some(&self.added),
            added: BTreeMap::new(),
        }
    }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.added
            .get(key)
            .or_else(|| self.base.and_then(|base| base.get(key)))
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.get(key).is_some()
    }
    pub fn insert(&mut self, key: K, value: V) {
        self.added.insert(key, value);
    }
    // Used only by the segment interner, which never replaces a key.
    pub fn len(&self) -> usize {
        self.base.map_or(0, BTreeMap::len) + self.added.len()
    }
}
impl<K: Ord, V> Index<&K> for Table<'_, K, V> {
    type Output = V;
    fn index(&self, key: &K) -> &V {
        self.get(key).expect("inventoried key")
    }
}
