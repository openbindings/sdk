//! Version-locked allocation estimates shared by SDK companions. All widths are
//! canonical 64-bit widths, including on wasm32. No allocator is inspected.

/// Conservative live storage for an insertion-only pinned std BTree.
pub fn tree(len: usize, key: usize, value: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let leaf = (16 + 11 * (key + value) + 7) & !7;
    if len <= 11 {
        return leaf;
    }
    let leaves = (len.saturating_add(1) / 6).max(2);
    let internal = (leaves - 2) / 5 + 1;
    leaves
        .saturating_mul(leaf)
        .saturating_add(internal.saturating_mul(leaf + 96))
}
/// Upper storage for the pinned BTree bulk builder's sorted, unique input.
pub fn sorted_tree(len: usize, key: usize, value: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let leaf = (16 + 11 * (key + value) + 7) & !7;
    let mut level = len.div_ceil(11);
    let mut bytes = level.saturating_mul(leaf);
    while level > 1 {
        level = level.div_ceil(12);
        bytes = bytes.saturating_add(level.saturating_mul(leaf + 96));
    }
    bytes
}
/// Core's lazy operation-index allocation, for the version-matched comparison
/// companion. This is an internal integration contract, not a document concept.
pub trait DocumentAccess {
    /// Newly allocated retained and temporary bytes; zero for an existing cache.
    fn operation_index_storage(&self) -> (usize, usize);
    /// Construct the index after the caller has reserved its estimate.
    fn prepare_operation_index(&self);
}
