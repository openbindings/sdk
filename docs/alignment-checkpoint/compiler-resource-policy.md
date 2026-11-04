# Independent compiler resource-policy recommendation

Review scope: the replacement contract and an executable first admission profile,
not production activation or a new universal memory/time guarantee. SDK production
is unchanged by this review. Earlier AST/HIR/NFA source certificates, failed
formulas and observed requests remain intact.

## Decision and authority

Approve the direction in `design/2026-11-02/compiler-resource-contract.md`.
Pinned core §9 requires taking untrusted size, regular-expression and recursive
traversal risks seriously; it does not prescribe source-word accounting or a
cumulative allocator-request certificate for third-party constructors. Neither the
pinned current interfaces nor the public evaluator outcome model prescribe these
SDK numeric options. The complete dependency ledger was an adopted, unreleased
SDK promise. Matthew's greenfield instruction permits replacing it explicitly.

Keep original schema/value semantics, no I/O, typed Limit distinct from Undefined,
first-cause/sticky shared attempt failures, cancellation at cooperative boundaries,
independent per-evaluation caches, and immutable prepared ownership. Required
semantic and consumer workload expectations remain requirements. Resource refusals
are never counted as their successful evaluation. Existing measured performance,
owner cleanup, host memory and physical-stack gates remain gates.

The new statement must be concrete: SDK work/reservation counters cover their
specified cooperative SDK sites. Dependency calls are admitted by a finite
structural profile and representation ceilings, with pre/post cancellation and
actual-build resource qualification. Neither category promises total process RSS,
an allocator-wide byte ceiling, an instruction count, a deadline or prompt
interruption inside an uncooperative dependency call.

## Minimal factory

1. Check the enclosing attempt and bounded source extent. Run the existing lexical
   expansion/class-pressure preflight **before Reader**. Keep the current
   `preparation_cost` protection initially; a later smaller structural reader
   replacement must retain its property/shorthand/class-union cross terms.
   `max_form` alone is too late: expansion and union scratch precede rendering.
2. Read once, with authored group ceiling 32 (caller may tighten). Preserve
   Invalid/Unsupported readings. Simple programs bypass AST/HIR/NFA and retain
   their existing independently bounded ownership/matching path.
3. For a Linear non-Simple form, run the allocation-free normalized guard. Admit
   normalized depth 33 and bounded AST node/range breadth **before** the parser.
   Parse one AST with representation depth 250 and run one streaming prospective
   HIR inventory. Keep only facts required by the selected structural policy;
   the old 21 work-site and 7 allocation-site arrays are not required API or
   architecture. Do not retain a width-sized metadata tree.
4. Before translation, check prefix-call depth 64, equality height 256 and the
   breadth/reconstruction limits below. Translate once with the fixed reviewed
   configuration, including UTF-8 enabled. Drop temporary owners and errors before
   the completion check. Do not retain source or the attempt in the program.
5. Inspect the actual owned HIR once, iteratively, for the named compiler-path
   bound (512), repetition-expanded breadth/helper payload and empty-tail
   pressure. These are structural quantities, not an exact dependency operation
   ledger. All arithmetic is checked or capped at the relevant finite limit+1;
   zero-times-overflow must remain zero. Count only actual reachable compiler
   branches, notably bounded-minimum-zero and the at-least-two-literal trie case.
6. Build one fresh forward capture-free NFA **once at the final per-program size
   ceiling**. Do not retry geometrically. In locked Builder, `set_size_limit`
   merely assigns the limit and calls `check_size_limit`; it does not reserve the
   allowance. Retries were useful to our old allowance-proportional reservation,
   not to actual tiny-program storage. Removing them eliminates repeated trie,
   Unicode and compaction work and a separate retry budget.
7. Check actual final NFA retained size, aggregate unique-program retained size,
   and the enclosing attempt before publication. The builder's approximate state
   limit is not the final retained size: the preserved 1 KiB build produced a
   1,212-byte final NFA. Include Simple/program/reading/index ownership in the
   explicitly named retained accounting, with sharing counted once.
8. Construct the PikeVM from that NFA, retaining no mutable search cache. Keep the
   already reviewed cache breadth formula (union edges as well as states), actual
   per-call storage reservations and matching-work accounting. Outcome must not
   acquire diagnostic seeds. Drop/retry/concurrent evaluation remains isolated.

Per-pattern structural/profile/representation failure is a stored local terminal
Limit, surfaced only when that predicate is demanded. It is not an uncertainty
atom and does not make preparation fail merely because an unused operand is
expensive. Actual shared attempt exhaustion remains a sticky preparation failure.
A required meta-schema that demands a limited predicate cannot be admitted.
This preserves the existing 6,000×`\\S` sibling-reason control. "Demanded" means
what the actual interpreter reaches; this change does not promise new logical
short-circuiting of every decisive branch.

## Concrete first profile for implementation experiments

These values are a proposed finite policy, calibrated against the preserved
independent summaries. They are **not yet an activation/resource PASS**. They
should live in one private supported profile; caller-selected lower bounds may
restrict it, but an arbitrary larger request cannot bypass the supported profile.
Do not publish dozens of AST/HIR implementation counters as new public API.

| Admission dimension | Per non-Simple program | Preparation aggregate proposal |
| --- | ---: | ---: |
| Authored/normalized group depth | 32 / 33 | not additive |
| AST representation depth | 250 | not additive |
| Guarded AST node upper bound | 16,384 | 262,144 |
| Normalized class items/ranges | 32,768 | 262,144 |
| HIR reconstruction entries | 262,144 | 4,194,304 |
| HIR literal-byte revisits | 1,048,576 | 8,388,608 |
| HIR candidate range slots | 33,554,432 | 67,108,864 |
| Prefix/equality logical depth | 64 / 256 | not additive |
| Actual HIR named compiler path | 512 | not additive |

For the guard, use the reviewed prospective bound
`N = literals + assertions + classes + 3*groups + repetitions + 3*pipes + 2`,
not the later observed AST node count. It includes temporary and empty roots.
Ranges count class items; byte spelling alone cannot replace this bound.

The current inventory can supply reconstruction entries as
`concat_visit + alternate_frontier + prefix_nodes + prefix_split`, literal-byte
revisits as `concat_bytes + prefix_bytes`, and candidate range slots as
`range_slots`. These are conservative structural pressure scores. They are not
elapsed work, bytes or guaranteed actual constructor counts. A later simpler
inventory may compute equivalent sufficient bounds directly. Do **not** preserve
`sort_pairs` as an exact execution charge: the independent flat-prefix-1000
estimate is 2,057,224,320 while its modest actual construction was already
qualified. Our purpose is finite breadth admission, not defending that loose
quadratic quota.

The range score intentionally has more headroom than the first rejected draft:
`generated-8311` has 17,293,248 candidate slots but measured only 128,841 bytes peak
and 373,472 requested bytes for the whole HIR factory; its earlier 9.47 billion
source-word estimate is not a measured cost. No adverse measurement is being
hidden by preserving this form. The still-required boundary qualification below
must challenge the accepted structural envelope, not just these easy inputs.

For actual-HIR admission, start with these smaller, named quantities rather than
porting the full NFA site certificate:

- repetition-expanded HIR dispatch count: 65,536/program, 1,048,576/attempt;
- repeated helper payload units (literal/trie bytes, alternation entries and UTF-8
  sequence/range entries): 1,048,576/program, 8,388,608/attempt;
- prospective empty-tail join pressure: 1,048,576/program, 8,388,608/attempt.

These last three are proposed next-source-review values, not already computed
calibration results. Their fold must be defined and frozen before implementation:
finite repetition uses max, unbounded repetition uses max(min,1), zero copies
must annihilate child cost, literal-only trie alternatives are one helper, and
shared/backward empty-tail edges use the preserved constructor-aware source
argument. The helper score must bound actual UTF-8 sequence enumeration and long
literal trie input even when the resulting NFA is tiny. Finite input alone is not
a reason to omit these pre-build checks.

For the first one-build experiment, retain the existing 32 MiB backend
per-program ceiling and 64 MiB aggregate pattern-storage policy as upper ceilings,
then enforce actual final retained size independently. Do not raise either. The
new structural profile may make their extreme values unreachable. There is not
yet sufficient measured boundary evidence to justify a smaller replacement
numeric ceiling; choose any reduction explicitly after the candidate-envelope
probe. A backend state ceiling is not a total transient-memory ceiling.

The aggregate counts debit each actual construction; reuse of an already prepared
identical normalized program does not recompile it. Do not refund failed,
cancelled or transient construction within an attempt. Global aggregate-cap
exhaustion is sticky. Local per-program failure must not spend nonexistent
translation/build work or taint unrelated operand outcomes.

## Calibration, preserved negatives and limits of this evidence

`calibrate.py` reads preserved independent oracle summaries; it does not obtain
expectations from a new evaluator or change any semantic oracle. It checks the
lexical node upper bound against every existing exact AST node count. Results:

- 1,186 frozen source rows: 44 prior Reader noncomparisons; 1,140 admitted;
  exactly the two independent large construction counterexamples refuse.
- All 18 independent inventory controls admitted.
- All 10,065 generated inventory summaries admitted.
- All 12 unchanged performance patterns admitted, including common-prefix-1024:
  node upper bound 10,242, reconstruction 4,098, literal revisits 14,336.
- The 1,024 frozen tiny programs have only about 15 guarded nodes and 10
  reconstruction entries each, fitting the proposed aggregate as well as the
  per-program limits. This is a shape check, not a full preparation PASS.

Preserve the initial 64 Ki reconstruction candidate and 128 Ki/1 Mi range-score
candidate classification files: the first unnecessarily refused flat-prefix-1000;
the second rejected 686 generated summaries because of conservative range scores.
The final candidate admits them all. This is explicit policy calibration, not
relaxing semantic expectations or claiming those failures were implementation bugs.

The 20,192-byte/depth32 counterexample previously requested 85,727,802 bytes
cumulatively with 7,139,808 bytes peak even at a 1 KiB NFA allowance. Its input and
old measurements remain unchanged. Both it and the 40,223-byte companion exceed
the new node/reconstruction profile; their new expected result is a **local
resource limitation**, with proof of no parser/HIR/NFA entry after the relevant
guard, not a semantic S/F/U success. The old unrestricted probe stays available
as a positive reproduction of the resource hazard.

Breadth alone is not the whole policy. `paired-prefix-4000` has only 16,005 actual
AST nodes (guarded bound 16,007) but reconstruction pressure 328,102, so it is a
separate pre-HIR refusal witness. `paired-prefix-3195`/3196 straddle the 262,144
reconstruction threshold. These are semantic patterns whose construction is
outside the proposed profile; do not call their refusal semantic conformance.

## Public contract and test disposition

Use explicit scope in field names, not an unchanged whole-construction promise
with an exception hidden later in documentation. The smallest Rust change is to
rename the two fields to `max_sdk_work_units` and
`max_sdk_reservation_bytes` in `PreparationLimits`. Keep their shared Attempt
ownership and nested first-cause behavior. Describe their covered cooperative
SDK operations/reservation sites; estimates may conservatively charge known
helpers, but success does not certify all dependency allocation or CPU activity.
Keep `max_schema_nodes` and `max_identity_bytes` as independently meaningful
structural limits. Do not add a second generic evaluator abstraction.

TypeScript/Wasm do not currently expose those two PreparationLimits fields. Do
not invent counterparts just for naming symmetry. Update Rust generic-evaluator
requirements, DefaultEvaluator docs, examples, errors and exact-quota tests
consistently. Custom evaluators must state their own applicable resource scope;
SDK counters cannot account for arbitrary callbacks, allocator hooks or external
provider work. The earlier Mutex recorder allocation failure remains evidence
of why that distinction matters.

| Existing obligation | Disposition |
| --- | --- |
| Pinned S/F/U, original locations, no-I/O, exact values | Preserve unchanged |
| SDK environment/identity/metadata/graph/storage exact and one-below tests | Preserve under the renamed scoped budget |
| Attempt nesting, ignored-error poisoning, cancellation, first cause | Preserve unchanged |
| Old dependency source-word/layout/cumulative-request totals | Preserve historical receipts; remove as universal API promise; replace active quota assertions with structural boundary/pre-entry checks |
| Constructor allocator/peak/time probes | Preserve inputs and raw results; keep as measured qualification rather than prove an arbitrary-build universal bound |
| 19 generated Wasm u32 source-word overflows | Preserve old outcomes; re-adjudicate under the new actual structural gate, never silently relabel them success |
| Structural hard-limit ±1 and original counterexamples | Add required current-policy controls, typed local Limit, no dependent backend entry |
| Whole-dependency `max_allocation_bytes` success claim | Remove; no rename may imply it still exists |
| Search-cache bounds and evaluation work/diagnostic budget behavior | Preserve their separately qualified scope |
| Comparison/source-pull budgets | Out of scope; do not weaken or rename them |
| Old geometric retry schedules and target layout tables | Historical evidence, no longer implementation requirements |
| `stack_bytes`/outgoing compiler options | Remove with the outgoing compiler, preserving the separately approved logical limits and recorded-build stack contract |

## Finite implementation/qualification sequence

1. Implement the single private structural profile and a lean owner-preserving
   inventory path, initially inactive. Retain current Reader preflight. Freeze
   the actual-HIR fold's small output record and independent boundary controls;
   do not implement the abandoned NFA work/allocation-site arrays.
2. Qualify one-build NFA plus actual retained storage, current/minimum native and
   actual Wasm. Replay required semantics, all retained pattern forms and matched
   performance workloads. Keep noncomparisons and new resource refusals distinct.
3. Add cap-boundary adversarial combinations: node×range, depth×breadth,
   prefix×equality, long equal/common prefixes, repeated Unicode helpers,
   nullable repetitions, empty-tail fan-in and nested repetitions. Include
   under-cap combined cases, not just each cap alone. Measure transient peak,
   cumulative requests, construction/search time and complete cleanup. Freeze
   acceptance ceilings/matched baseline tolerances **before** judging this lane;
   current source summaries alone do not justify a portable numeric transient cap.
4. Exercise local masking/sibling reasons, required-meta demand, aggregate
   exhaustion with already retained programs, exact and one-below limits,
   cancellation before/after every uncooperative stage, cleanup after each
   failure, concurrent evaluation, and zero retained attempts. Preserve all
   existing ordinary 20k/20,001 array, 50k object and large reference jobs.
5. Only after these pass, move the once-prepared program path into the single
   original-node engine and remove the outgoing general compiler and obsolete
   quota options/tests together. Full pattern preparation and default-engine
   alignment remain open until that happens.

The approved result here is a smaller honest contract and a concrete candidate
profile/sequence. It does not certify the candidate's full resource envelope,
physical stack on arbitrary Rust builds, or complete original-engine activation.
