# Pattern search admission

The default ceiling remains 250,000,000 SDK work units for one uncancellable
Original search. These are conservative algorithmic work units, not elapsed
time or processor instructions. No dependency is modified or newly enabled.

## Capture-free PikeVM

The pinned regex-automata 0.4.18 implementation is
`nfa/thompson/pikevm.rs`: `is_match` uses `earliest(true)` and `search_slots`
with no capture slots. `search_imp` processes at most input bytes plus one
positions. `nexts` visits each active state once per position.
`epsilon_closure_explore` inserts into a sparse visited set before expanding
a state, so each state's outgoing epsilon edges expand at most once per
position. Duplicate insertion attempts are covered by their incoming edges.
The engine has no prefilter here. Capture copying/restoration has zero slots.
The Reader emits only scalar-consuming atoms, start/end assertions and ASCII
word boundaries; it refuses the non-word-boundary form that can split UTF-8.
Consequently the empty-match UTF-8 split filter does not restart these searches.

The retained bound is the sum of four units per state plus four per outgoing
edge, with one unit per position for the outer loop. Binary unions have two
edges. Dense and byte-range transitions follow at most one edge. Look assertions
retain the conservative 32-edge allowance except Start and End, which each
perform one offset comparison and follow at most one edge.

Sparse transitions are different: `SparseTransitions::matches_byte` in
`nfa/thompson/nfa.rs` scans ranges, but follows at most one edge. A range costs
at most three comparisons (the ordered-scan check and both interval endpoints).
The eight-unit byte-range allowance contains two comparisons and six units for
state/edge processing, including closure insertion and stack bookkeeping.
Thus `6 + 3 * ranges` bounds a sparse state's work. We use the smaller of this
bound and the existing `4 * (ranges + 1)` bound. This does not assume binary
search, compiler removal of redundant comparisons, or favorable input bytes.

For a start-anchored NFA, there is only one start closure. The existing HIR
`maximum_len()` bounds every consuming path. When finite, at most that many
bytes plus one delayed match step can be visited before the active set empties.
We therefore charge `min(input_bytes, maximum_len + 1) + 1` positions. The
extra position is conservative. Unbounded or unanchored programs retain the
input-length bound. Computing the HIR property requires no additional traversal.

For an unanchored program whose full-input bound exceeds the ceiling, the one
search uses a prefix span of at most `ceiling / weight - 1` bytes. The engine
still receives the complete haystack, so end and word-boundary assertions do
not see an invented boundary. A match in that span proves a whole-input match.
No match in a shortened span returns `pattern-search-work-limit`, never Fails.
There is no retry or uncharged preflight search. Anchored hostile patterns are
refused before engine entry rather than spending the ceiling on a prefix.

## Simple matcher

The existing normalized-form weight remains unchanged. Its finite inspection
bound comes from the actual matcher: literal steps inspect their byte count,
other fixed steps consume at most four bytes per scalar, and a finite final
run reads at most its maximum plus one scalar. Without an end anchor the last
run only reads its minimum. An unbounded final run retains the full input bound.
The table fast path represents the same fixed steps. A final end check reads
the remaining slice's length, not its contents.

Both bounds are computed once during preparation. The per-value charge uses
constant-time arithmetic. Cancellation and cumulative work charging keep their
existing precedence. Retained program and allocation accounting include the
additional inline inspection bound.

## Public regression corpora

`tests/fixtures/pattern-work-ordinary.json` specifies expected verdicts with a
250,000,000-unit ceiling and a target of zero false NoVerdicts.
`tests/fixtures/pattern-work-hostile.json` specifies rejection at that same
ceiling. `tests/pattern_search_work.rs` runs both through supplied roots and
document operation inputs, using both evaluate and explain. There is no
elapsed-time assertion. The hostile bound prevents engine entry; cache setup
is separately bounded by the existing work and storage controls.

Explanation first uses the same lane selection as evaluation. If the compiled
lane establishes failure, one Original diagnostic pass may enrich it. Exhausted
diagnostic work, storage or cancellation leaves that failure intact, with
incomplete problems. Nondefault limits already select Original for both calls.
