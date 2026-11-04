# Original pattern search cache reservation

Independent design review approved the existing call-local PikeVM correction,
including explicit per-vector layout limits. Parent and independent implementation/
resource qualification pass on pushed `421706c`. Full immutable program preparation remains under separate review. This correction does not move compiler
construction, change the supported pattern language, or preserve an obsolete API.

The locked regex-automata 0.4.18 PikeVM stores pending epsilon branches separately
from its active-state sets. An accepted pattern containing 1,000 `|` characters
has only four NFA states but 1,003 epsilon edges. The existing `256 * states +
4096` reservation is 5,120 bytes; independently measured cache construction and
search request 16,192 bytes. A state count alone cannot bound the pending stack.

## Ownership and accounting

Keep the engine/cache pair local to one evaluation for this bounded correction.
Compute a requested-allocation upper bound from the completed immutable NFA,
charge it before `create_cache`, and reuse that same reservation for every search
using this cache. Cache vectors never shrink, reset to another engine, or clone.
A clone would need a separate reservation. Overflow, including addition to prior
Graph reservations, refuses before cache construction. Each possible vector backing
must also fit `isize::MAX` before entering the dependency. Charge work for the state scan and check the
caller control again before allocating.

The locked implementation has four `Vec<StateID>` backing arrays (two SparseSets)
and two `Vec<Option<NonMaxUsize>>` slot tables. Each starts empty and is resized
once. For `N` states, `G` capture slots and `P` patterns, reserve:

- `4 * max(N, 4) * size_of::<StateID>()` for the sparse arrays;
- `2 * max(N * G + max(G, 2 * P), 4) * size_of::<usize>()` for slot arrays.

Although current construction guarantees one pattern with captures disabled, use
the actual NFA slot/pattern counts so the storage proof does not silently depend
on that constructor configuration. All additions and multiplications are checked.

In one epsilon closure, the visited set prevents expanding a state twice. A Union
follows its first edge inline and pushes each remaining edge; BinaryUnion pushes
one edge; Capture can push one restoration frame. Thus a conservative maximum
pending count is `L = 1 + sum(max(union alternatives - 1, 0)) + binary unions +
capture states`. Cycles and repeated destinations do not invalidate this count:
each state's outgoing branches are added at most once per closure. The stack is
drained before another closure and retained between searches.

The private FollowEpsilon enum contains a discriminant and at most a SmallIndex
and Option<NonMaxUsize>. Three machine words bound its stride on the supported
32/64-bit Rust targets. Vec grows to at least twice its old capacity, or directly
to a larger required count, with minimum initial capacity four for this element
size. Across all such growths, the sum of requested capacities is less than twice
the final capacity; that final capacity is at most `2 * max(L, 4)`. Reserve
`4 * max(L, 4) * (3 * size_of::<usize>())` for all requested stack backing,
including reallocations. This bounds cumulative requests, not just final live
bytes. The bound is deliberately conservative and independent of the haystack.

Dependencies and toolchains are part of this proof: re-audit the enum layout and
Vec growth if they change. Allocator metadata remains outside the SDK's existing
requested-byte policy. The dependency's `Cache::memory_usage()` measures stack
length rather than retained capacity, so it is not an adequate validation meter.

## Validation gates

Use original pattern readings with 100/1,000/10,000 empty alternatives, nested
alternation/repetition, wide real branches and byte/Unicode cases. Independently
measure construction plus multiple searches and drop with an allocation meter:
requested bytes must not exceed the schedule, retained owner bytes must return to
zero, and later searches may grow but cannot receive a fresh uncharged allowance.
Verify the cache stage at its exact reservation and one byte below, cancellation
before its scan/creation, arithmetic overflow, and original positive/negative
matching. Run forced-original conformance, current/minimum Rust, and actual Wasm
hosts for the change. Record dependency/source hashes and independent review.
