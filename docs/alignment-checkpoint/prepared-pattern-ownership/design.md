# Once-prepared pattern ownership

This replaces the architecture portions of the earlier full-certificate schedule.
The pinned spec and interfaces govern meaning; the reviewed structural resource
contract governs construction. There is no compatibility requirement for existing
private types, public option names, old cache divisions or diagnostic prose.

## Owners and APIs

`semantic::patterns::Program` is the immutable interpretation retained by a Plan:
Undefined (invalid syntax), Unsupported (a known capability gap), Simple (owned
matcher plus match-work bound), Vm (PikeVM plus match-work bound), or Limited
(static local resource/representation cause). It holds no normalized/source text,
Control, Construction, HIR, mutable search cache or instance. Occurrence identifiers
and provenance stay in the Plan's occurrence-to-Arc<Program> map, not in Program.

`Construction` owns the one structural Budget and one temporary program index.
Its immutable evaluator Limits are selected once when Construction is created;
root, secondary and selected-meta Plans take their limits from this owner rather
than accepting different compiler policies with the same index. The program index
uses RefCell only around BTreeMap lookup/insertion and Cells for scalar counters;
no Reader/compiler/Control callback holds an index borrow. Its normalized String
to Arc<Program> index is shared only while
preparing this immutable environment. Different original spellings, resources,
pattern values and patternProperties names may reuse one normalized program.
There is no process-global cache. Distinct Undefined/Unsupported results need no
source-key retention. Neither a Plan nor a Program retains the index/Control.

`Construction::pattern(source)` preflights and reads under the current SDK Attempt,
then looks up the completed normalized form. A pre-normalization source/depth
refusal never enters that index under a partial key. Only completed readings and
deterministic local compiler refusals may be indexed; shared Attempt/aggregate
failures never publish an entry. Only a miss enters the compiler and structural
Budget. Source byte and authored depth checks continue to run before reuse. The
existing pre-Reader expansion/class-union protection stays in place. Simple owns
its matcher directly and bypasses all backend stages. A non-Simple miss calls the
joined one-build factory, then computes the existing state/edge match-work bound
once. SDK-owned walks/index/owner reservations remain cooperative Control charges.
Check shared failure/cancellation before publication and after temporary cleanup.
A separately charged temporary normalized key allows transactional insertion
rollback after late cancellation; one index String remains, and neither string
enters Program/Plan. This reviewed refinement avoids introducing another retained
key owner solely for rollback.

`patterns::Search` owns Arc<Program> and optional mutable Cache for one reached
operand in one value call. It never parses or compiles. A Vm creates its cache
using the already reviewed union-edge-aware reservation. Search work remains
instance length times the prepared state/edge bound. Concurrent calls have separate
caches and Controls. An occurrence-based per-call table retains existing effect
identity; equivalent occurrences need not share mutable caches.

## Limits and local outcomes

The private per-program NFA ceiling stays 32 MiB. The enclosing aggregate upper
ceiling stays 64 MiB and can be lowered by the caller. The retained footprint is explicitly defined as owned Simple backing,
locked NFA memory_usage (representation lengths, not allocator capacities), and
separate Arc/Program ownership overhead once per created program. This is not
actual allocator bytes; measured host requests/peak/retention remain separate gates. The
temporary normalized index has its own source-form extent bound and SDK reservation;
it is not misdescribed as retained by the completed Plan. All index and occurrence
storage is also subject to SDK-owned reservations. Fixed arithmetic uses checked
addition; failure is sticky at outer completion. Program memory is not divided
into guessed 48 KiB compiler slots or reserved search-cache slices.

For a non-Simple program the build gets the fixed supported per-program ceiling,
not a geometric retry and not a repeatedly renewed share of aggregate memory.
After build, retained unique-program bytes are checked before insertion. Actual
aggregate retention exhaustion is a whole-preparation Limit, with no partial set
published. Entered structural phases keep their debits even when a local build
fails. Programs held for a selected meta-schema also contribute to the enclosing
construction; dropping a temporary Plan does not refund the attempt.

Per-program source-byte, authored-depth, backend-profile, expanded-form and
representation refusals are stored as Limited and surfaced only when reached.
When the Reader cannot complete within the selected local limit, the SDK makes
no claim that the unexamined syntax is valid or invalid. Old tests that required
whole-preparation refusal for these local limits are policy tests: re-adjudicate
their timing explicitly without weakening demanded Limit outcomes. Demand is actual interpreter reach, not a new guarantee that deciding anyOf/if
branches short-circuit every sibling. It is terminal Limit, never Undefined/uncertainty, and does not
poison unused or nonstring sibling outcomes. A selected meta-schema that demands
a Limited predicate blocks preparation. Invalid syntax and unsupported features
retain their separate meanings. Shared Attempt/structural exhaustion always fails
the enclosing preparation. The 6000-shorthand sibling-reason witness stays intact.

## Sequence and gates

1. Finish and pin the joined factory/aggregate checkpoint, including bounded
   independent review, native current/minimum tests and actual Wasm qualification.
2. Review this ownership/API seam; implement Program/index/Search in the original
   evaluator route. Preserve the outgoing default only until replacement gates
   pass, not as a compatibility facade. Remove the old per-value retry loop now.
3. Exercise AGG-15 through AGG-24 from the independent aggregate control set on
   actual root/meta routes. Prove normalized reuse, one build across repeated values,
   unchanged occurrence reasons, no retained construction, cancellation/retry and
   independent concurrent searches. Exact aggregate/one-below controls must measure
   the new explicitly named footprint, not reproduce old division arithmetic.
4. Replay unchanged 100/300/400/1024 tiny-program jobs and source/Unicode/nullable/
   repetition/shape adversaries under full defaults. Qualify observed transient
   memory, cumulative requests, time and native/minimum/Wasm stack on recorded
   builds. Refusal is not counted as successful semantic evaluation.
5. Force the original evaluator through semantic/consumer/reference and actual-host
   suites. Resolve findings, switch the default, delete the outgoing engine and
   obsolete options/types/docs, then run final clean-package, performance, all-host
   and independent spec-to-code alignment gates.
