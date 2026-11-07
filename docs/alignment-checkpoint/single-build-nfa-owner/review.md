# Bounded inactive NFA owner review

Status: PASS for the inactive single-build owner and named-path guard at the source identities in `source-hashes.json`. This is not approval of compiler activation, the new `admission` fold, aggregate construction, all-pattern transient bounds, or final default/performance/physical-stack policy.

The owner checks the caller's existing failure first, measures the original HIR with an ancestor-only traversal, rejects a path above 512 before backend entry, and performs one fresh forward UTF-8 capture-free build at the supplied representation ceiling. It checks actual returned NFA `memory_usage` separately from the backend's approximate build ceiling. Successful PikeVM ownership retains neither the HIR, control, compiler nor search cache. Local Size/Representation/ConstructionLimit does not poison the enclosing Attempt. Post-call checks preserve cancellation or the earlier shared failure before publishing a program or local refusal. No retry machinery was introduced.

The path calculation preserves the independently frozen helper branches: min-zero bounded repetition, exact zero, general alternation, and at least two literal alternatives for the iterative trie. Prefix arithmetic is bounded by the checked 512 path before descent. Sibling storage grows only along ancestors; each requested new backing extent is reserved before growth. The path is a named compiler-descent limit, not a physical-byte stack claim.

## Independent checks

* Current Rust 1.99 and minimum Rust 1.88: 195 owner controls each, including 15 existence/anchor/Unicode/nullable/repetition cases at zero, tiny and funded representation limits; every observed cancellation checkpoint; retry; actual allocator cleanup. All completed with zero live construction allocation after drop. `results.json` records the observed maxima without treating them as a universal quota.
* 16 frozen helper-path rows plus synthetic 511/512/513 guard boundaries: all 19 pass. The synthetic positives test the guard alone, not admitted source patterns or arbitrary native stack support.
* Injected allocation failure proves the corrected ancestor-reservation branch returns local Size normally and Attempt(Cancelled) when cancellation happens in that allocation. A reconstructed old branch returns Size in both cases. See `allocation-stop-evidence-note.md` for the preserved chronology: the unfortunately named before snapshot/log already contained the fix; no claim is made that an old failing binary was retained.
* Source review confirms exact retained-size/one-below and no-retry unit controls, independent per-thread search caches and prior shared failure precedence. Parent unit/host results are supplementary and are not counted as independently rerun here.

The path-only copied probe initially failed because a newly added uncalled sibling module was outside the copied module directory. Its final bridge omits only that declaration and appends a private path observer; the production owner code is unchanged. Original compile failure remains in `path-build.log`.

## Finding closed

The original direct mapping from failed `try_reserve_exact` to local Size could hide cancellation raised during the allocation. The added `control.check()` on that error branch closes the issue. Both injected positive and reconstructed negative evidence are retained. No additional owner blocker was found.

The outer once-prepared factory must still supply upstream Reader expansion, AST/HIR breadth/reconstruction and the independent NFA score admissions; debit the shared structural aggregate before dependency entry; and account unique retained program ownership before publication. The current module expressly documents these preconditions and remains inactive.
