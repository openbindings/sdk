# Post-alias root identity before sharing flag — bounded design GO

This is a read-only source review of the corrected State-copy/root-memo candidate over f62. No builds, tests, measurements or SDK edits were performed for this disposition.

Inside the existing `if fields.is_leaf()` branch after alias normalization and pure-cycle handling, the following reordering is sound:

```rust
if backend::ref_id(value) == self.root_instance && fields.is_shared_leaf() {
    // unchanged root_leaf call
}
```

`backend::ref_id` returns the borrowed value's arena position. `is_shared_leaf` only tests the copied Fields word (including its leaf/original bits). Neither allocates, mutates, invokes a callback, charges an allowance or returns an error. Reordering therefore preserves the same true/false result, every SDK work/storage/control event, the same subsequent alias-target leaf evaluation or memo path, and all source/context/cut/witness identities. The root instance is still taken from the actual value view for this Run, including a nonzero subtree root. Property-name and child-value occurrences remain distinct within the one retained instance arena.

Keep the condition exactly at this point. The alias walk, scope entry, cycle handling, original source selection, fields lookup and outer certified-leaf test remain necessary. Do not move the early rejection ahead of those operations, and do not skip child evaluation or infer absence/certification from the instance check.

For non-root child instances the reordered condition avoids the **additional shared-leaf predicate**. It does not avoid the already-completed Fields lookup or outer `is_leaf` check. Fields is an inline copied u64; this is not a second map/cache lookup. Its realized cost depends on emitted code. Previously an unshared leaf could skip the root-ID comparison; reordering performs that comparison first and may shift tiny costs among inputs. Static correctness is established; a performance gain, elimination of the negative-workload regressions, or G6 acceptance is not.

The exact root-ID fact is already the simplest sound available exclusion. An empty dynamic context, active/frame depth, zero arena index, scalar kind or query count is not equivalent: logical/ref applications may retain the same root value at several frames; a public root can be a retained nonzero subtree; child values can have empty context. Rewriting into an early child return expresses the same branch and duplicates the existing leaf continuation, with no stronger fact or clear benefit. No new metadata, cache, special-case identity or storage policy is warranted.

A retained implementation should be checked as a one-line diff, with the existing same-root/shared and different-root-instance negatives; pure predicate reordering warrants no changed outcome, charge or callback oracle. Selected repeated-root, single-chain/unshared, two-child, 20k-array and 50k-object measurements remain the decision evidence. Prior State-copy and root semantic receipts continue to apply because their data structures and substantive branches are unchanged.

The earlier G5 naming recommendation is at `limits-terminology-note.md` in this directory. It proposes a separate scoped `evaluation_sdk_work_units` / `evaluation_sdk_reservation_bytes` cleanup, retaining numeric defaults and semantic behavior; it is not part of this gate-order experiment.
