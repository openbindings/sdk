# Prepared exact numeric operands

Continue the single original-node engine design by preparing schema-side numeric
operands once within a bounded optional cache. Oversized or excess operands keep
original evaluation. Runtime values remain caller-specific and exact. This private
representation follows the supported numeric/count keywords; it creates no new
public schema concept or compatibility requirement.

For selected activated original nodes/profiles, group the five numeric predicates
(minimum, maximum, exclusiveMinimum, exclusiveMaximum, multipleOf) as exact
numeric::Bound readings or explicit original fallbacks. Retain source-byte lengths
for bounded comparison/arithmetic. Read the six length/size bounds and applicable
minContains/maxContains as exact nonnegative u64-saturating counts or original
fallbacks, matching the arithmetic meaning of sizes the host can hold.
Use original node and keyword identity; context/resource snapshots never share an
implicit URI cache. No borrowed preparation control survives.

Charge source scanning, exact parsing and map/record ownership before construction
under the same preparation attempt. Invalid shapes/invalid multipleOf operands
remain local typed inability readings rather than becoming new blanket preparation
failures. An inactive vocabulary or opaque keyword never creates an operand. An
absent bound remains absent, and orphan contains counts are not value assertions.
Actual exhausted preparation allowance remains sticky. Do not silently renew
construction or arithmetic budgets or enlarge defaults.

Per-value numeric evaluation borrows prepared bounds. Keep exact instance parsing,
big-integer scratch/work bounds and the explicit arithmetic capability refusal.
Prepared counts are copied and prepared numeric text is not reparsed on each item
or invocation; fallback operands keep their charged original reading.
The current predicate effect identity and independent-branch behavior remain intact.
This increment does not yet prepare enum/const structural identities, change
instance equality, or replace iterative frame/diagnostic work.

Controls: exact decimal/exponent aliases and large numbers; every comparison and
multipleOf with positive/invalid operands; size/count zero, u64 boundary and larger
valid counts; permissive custom meta admitting malformed operands; inactive
vocabularies; orphan counts; missing resources/independent branches; default large
containers; preparation and evaluation limits/cancellation/retry; original owners
and concurrent prepared calls. Direct Plan controls must prove hot calls avoid
schema parsing while existing semantic/host corpora remain unchanged. Independently
review admission timing of large unused operands before accepting this increment.

## Reviewed admission and runtime corrections

Independent design probes found two concrete hazards in unconditional preparation:
2.5-million and 3-million digit unused minimum operands prepare and evaluate an
empty object today, but the existing 8N+1 parse reservation alone would exhaust
the default 20-million preparation-work allowance. A cached bound also does not
eliminate bound-dependent comparison cost: comparing a one-byte value against a
bound with an N-digit exponent requests 3N+3 bytes, with 2N+2 incremental peak,
through retained exponent magnitude arithmetic. Preparing such a bound does not
justify deleting its runtime reservation.

The first implementation therefore uses a bounded eager subset and an explicit
original-operand fallback. Per Plan, prepare numeric tokens of at most 256 bytes,
with at most 65,536 aggregate source bytes and 1,024 retained original-node
records. These are private optimization thresholds, not schema capability limits:
exceeding any selects original evaluation before making the optional parse charge.
A record groups the present numeric predicates, size predicates and applicable
contains counts for one original node. Values outside the eager subset remain
explicit original readings; absence of an entire record means read the original
node, never that its predicates are absent. Do not prepare orphan contains counts.

Reserve each retained record and parsed operand before allocation. Invalid
small operands retain a local invalid reading and produce the same typed inability
when the predicate is evaluated, rather than refusing unrelated preparation.
Charge all actual selected eager work to the same attempt; once any charge fails,
the preparation fails and cannot reset the allowance or switch to fallback.
The optimization does not promise identical near-limit admission cutoffs: added
retained records and eager work remain charged. No default increases are allowed.
The giant-unused-operand controls must remain admitted at defaults.

Runtime numeric comparisons retain the current schema-dependent work and scratch
reservation even for prepared bounds until comparison is separately audited to
avoid retained-exponent allocation. Instance parsing and bounded multipleOf
scratch also remain charged. The first benefit for numeric bounds is removal of
repeated parsing, not a claim that comparison allocates nothing. Prepared size and
contains counts are copied directly; those must remove repeated decimal parsing
and schema-dependent allocation at the reached count predicate. Grouped records
avoid a separate cache lookup for every possible keyword. Original fallback keeps
all current accounting and predicate applicability.

Acceptance adds: eager-threshold and aggregate-threshold boundaries; a node beyond
the record cap still evaluates every original predicate; warm calls with prepared
counts succeed under a work allowance that excludes reparsing; repeated/concurrent
calls and cancellation/retry; the exact huge-exponent allocation witness remains
accounted; large unused minimum/maxLength and orphan minContains remain admitted.
Review receipts: `original-engine-numeric-review/design/` under preparation receipts.
