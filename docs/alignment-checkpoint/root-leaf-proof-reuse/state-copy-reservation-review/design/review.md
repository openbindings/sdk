# Empty State copy reservation — bounded design review

**GO for this separate, explicitly recorded storage-policy refinement.** Keep root memo retention pending the corrected 64KiB control and implementation/allocator qualification. No SDK edits, builds or probes were performed by this review.

The current root-memo candidate has a real low-budget regression: the unchanged 5000-reference request satisfies under clean f62 with 64KiB, while the candidate returns semantic-analysis-memory-limit. Preserve that evidence. A default allowance or outcome waiver is not justified.

## Source justification

State contains a formula, Option<Arc<WitnessSeed>>, and two BTreeMaps. The exact pinned Rust 1.88 and 1.99 BTreeMap::clone implementations explicitly branch on `self.is_empty()` and return `BTreeMap::new_in(Global)` without constructing a root node. This also covers an empty map that previously contained entries; the proof does not need an insertion-only State invariant. Formula copying and the default-global Arc clone add no owned heap allocation. The Arc only shares the existing failure seed; no source text, pointer or other owned payload is copied. A State value itself is returned in already-accounted stack/frame/caller storage, not newly boxed by this clone.

Therefore removing a 256-byte heap-reservation estimate from a copy whose **both maps are empty** is justified. It is a global correction to the meaning of State copying, not a special exemption for the benchmark/root cache. This explicitly supersedes the earlier root-memo design's requirement to preserve that conservative base; it does not silently change a previously frozen implementation constraint.

The proposal must keep the exact existing sequence: initial Graph.work(items+properties+1), each property-name work charge, and Graph.charge(bytes), including when bytes is zero, then clone. Keep the nonempty base 256 and all per-entry/name terms unchanged. Do not skip charge(0): under a preparation Attempt it still executes checks, parent charging and sticky-failure observation. No callbacks, work units, defaults, failure mappings, eager child requests or semantic oracles are otherwise altered. Funded paths retain their ordered control events; explicit storage thresholds for empty states intentionally change.

## Required boundary

The refined byte formula is base 0 only for properties.is_empty() && items.is_empty(), otherwise base 256, followed by the current item/name terms. A map containing entries whose formulas are FALSE is nonempty and keeps its existing charge. Empty maps plus a witness still qualify: Arc sharing is not payload duplication. Nonempty maps with a zero-length name do not qualify, because the tree itself is cloned.

Use actual current/minimum allocator controls to confirm zero requests for empty copies (including prior insert/remove and witness sharing), and unchanged allocation/cleanup behavior for nonempty copies. Source evidence supports this narrow case; it does not create a new claim that every SDK reservation equals actual allocator bytes, nor reopen dependency-construction certificates.

`controls.json` freezes nine groups for the four map shapes, exact SDK charges and work, event/cancellation order, nested sticky/overflow cases, witness/effect ownership, actual allocations and the unchanged 64KiB witness. Per-call and selected-meta/Explain ownership remain as previously reviewed. Re-run the low-budget positive with the same request and retain the original failure. No root optimization or broader performance acceptance is granted merely because this unnecessary reservation has a sound correction.
