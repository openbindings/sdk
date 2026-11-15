//! Reservations for live collection backing. Capacity changes are deterministic
//! SDK operations; no allocator is inspected. The old table is released only
//! after reserve has replaced it. Reallocation overlap is included for hash
//! tables, whose replacement uses a separate allocation.
use openbindings_internal_schema::{Control, Failure};
use std::collections::{HashMap, HashSet, VecDeque};
fn buckets(capacity: usize) -> usize {
    if capacity == 0 {
        0
    } else if capacity < 8 {
        capacity + 1
    } else {
        capacity / 7 * 8
    }
}
fn bytes(buckets: usize, slot: usize) -> usize {
    if buckets == 0 {
        0
    } else {
        ((buckets * slot + 15) & !15) + buckets + 16
    }
}
pub(in crate::semantic) fn map<K: Eq + std::hash::Hash, V>(
    map: &mut HashMap<K, V>,
    extra: usize,
    control: &dyn Control,
) -> Result<(), Failure> {
    let old = buckets(map.capacity());
    if map.len() < map.capacity() {
        return control.charge_allocation(extra);
    }
    let slot = size_of::<(K, V)>();
    let next = bytes((old * 2).max(4), slot);
    control.charge_allocation(next.saturating_add(extra))?;
    map.reserve(1);
    control.release_allocation(bytes(old, slot));
    Ok(())
}
pub(in crate::semantic) fn set<K: Eq + std::hash::Hash>(
    set: &mut HashSet<K>,
    control: &dyn Control,
) -> Result<(), Failure> {
    let old = buckets(set.capacity());
    if set.len() < set.capacity() {
        return control.charge_allocation(0);
    }
    control.charge_allocation(bytes((old * 2).max(4), size_of::<K>()))?;
    set.reserve(1);
    control.release_allocation(bytes(old, size_of::<K>()));
    Ok(())
}
pub(in crate::semantic) fn push<T>(
    values: &mut Vec<T>,
    value: T,
    control: &dyn Control,
) -> Result<(), Failure> {
    let next = if values.len() == values.capacity() {
        (values.capacity() * 2).max(4)
    } else {
        values.capacity()
    };
    control.charge_allocation((next - values.capacity()).saturating_mul(size_of::<T>()))?;
    if next > values.capacity() {
        values.reserve_exact(next - values.len());
    }
    values.push(value);
    Ok(())
}
pub(in crate::semantic) fn enqueue<T>(
    values: &mut VecDeque<T>,
    value: T,
    control: &dyn Control,
) -> Result<(), Failure> {
    let next = if values.len() == values.capacity() {
        (values.capacity() * 2).max(4)
    } else {
        values.capacity()
    };
    control.charge_allocation((next - values.capacity()).saturating_mul(size_of::<T>()))?;
    if next > values.capacity() {
        values.reserve_exact(next - values.len());
    }
    values.push_back(value);
    Ok(())
}
