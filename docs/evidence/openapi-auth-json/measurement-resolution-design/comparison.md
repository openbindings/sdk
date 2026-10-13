# Independent proposals and synthesis decisions

The coordinator saved and committed its proposal at 1d2f142 before opening Claude's
new design. Claude's proposal states it did not read the coordinator proposal. Both
used shared prior evidence/history; this is independent proposal authorship, not a
claim of independent underlying data. No timing or implementation ran.

| Concern | Coordinator proposal | Claude proposal | Synthesis |
| --- | --- | --- | --- |
| Identify object effects | Same object; distinct objects with shared fixture storage; distinct objects with separate fixture storage | Same object versus independently constructed objects | Keep all three identities; they separate avoidable payload allocation differences from provider state differences |
| Retain comparable warm objects | Keep instances alive across samples and mapping permutations | Reconstruct every sample | Keep instances alive; crossover should compare the same physical objects rather than average newly allocated instances |
| Switching overhead |64 complete timed calls per large block |8 timed calls after one untimed switch call | Time every block call;64 large /256 small, chosen by payload class, with no per-block lead-in |
| Cyclic transition balance |16-symbol cycle including self transitions |12-symbol cycle with all distinct transitions |16-symbol cycle; every ordered predecessor pair appears once, including boundaries and self transitions |
| Construction and label controls | Independent label crossings; construction orientation across4 processes | Linked per-sample Latin rotations,3 processes | Retain3 processes, include both construction orientations within each; retain physical objects during normal/reversed label mappings |
| Prevent hidden bias | Per-cell direct, adapter and residual guards; empirical margin screen | Per-cell guards plus a300 ns pooled mean-bias check | Per-cell guards and component-based margin screen; avoid a global signed mean that can cancel between layers/editions |
| Bound scope | Broad null-only144 configuration matrix | Real/null/same-object rows plus new1 KiB AND attribution rows | Null-only 96 configurations; no new low-payload rows or exploratory real-auth repetitions |
| Verify the instrument | Identity/source checks and raw validation | Explicit validator self-tests, preflight-only first launch, in-run null sentinels | Include all three; don't label a preflight-only launch a benchmark or claim it removes OS/thermal effects |
| Handle borderline passes | Inconclusive if slack does not exceed component envelope | Compare slack to residual and allow explicit acknowledgement | Use component envelope; no discretionary acknowledgement bypass |

## Where Claude is stronger

Its proposal is more economical and gives sharper execution mechanics: a preflight-
only first launch, deterministic validator tests with corrupted records, explicit
in-run validity sentinels, and a stop after this bounded method attempt. These make
qualification harder to misreport and are adopted. Its smaller total run is a useful
check on the coordinator's tendency to expand the control matrix.

## Where the coordinator is stronger

The shared-fixture/distinct-instance contrast is absent from Claude's design. It
removes a known experimental difference in caller-owned immutable input/response
storage while preserving all per-call conversion work. Holding objects alive across
label swaps also preserves the identity needed for the comparison. Rebuilding every
sample can average over instance variation without explaining whether the same
instance retains an offset.

The coordinator's larger timed blocks preserve the switch-affected call instead of
excluding it more frequently. Its margin screen addresses a real defect in a simple
"nulls under1 µs, therefore any numerical pass is resolved" rule: a442 ns margin does
not become convincing just because a900 ns control difference is allowed.

## Corrections and tradeoffs

1. **Claude's proposed rotations are not independent.** For a fixed role r, setting
   slot rotation to (k+p)%4 and construction rotation to (k+p+2)%4 yields only 4 of 16
   possible(slot,construction-rank)pairs. For role 0 these are(0,2),(1,1),(2,0),(3,3).
   The offset does not decouple them. The synthesis crosses a retained object's label
   mapping with both construction orientations explicitly. It does not claim a full
   factorial over all8object allocation ranks or prove a microarchitectural cause.
2. **A pooled300 ns mean can conceal opposite offsets.** Individual1 µs checks remain
   useful, but a new signed mean across versions/layers is not a substitute for a
   margin/resolution check in each row. A new arbitrary bias constant is unnecessary.
3. **Predecessor-associated differences and the old/new absolute median difference
   are descriptive.** They do not establish that switching caused exactly3µs of
   additional runtime. Likewise a first-launch explanation is unproven. The new
   controls test hypotheses without treating them as facts.
4. **The coordinator's full independent D/C label crossing is expensive.** The
   synthesis uses simultaneous within-path reversal00/11, with component guards and
   the complete predecessor cycle. That is enough to test whether a retained pair's
   offset follows the physical instance or label under both construction orientations.
   It does not isolate every interaction obtainable from the omitted01/10mappings.
5. **More controls increase conservative inconclusive outcomes.** This is accepted
   and explicit; results will not be pooled to rescue a failed cell. The synthesized
   run remains bounded by fixed process/matrix counts and a hard timing cap.
6. **A small-payload AND row is unmeasured today.** It cannot establish the resolution
   of the required64 KiB gate, so adding it now would consume scope without answering
   the primary instrument question. No payload substitution or new product work.
7. **No statistical guarantee is invented.** The null envelope is a conservative
   observed-data screen, not a confidence interval. It neither corrects auth timings
   nor changes acceptance budgets. Smaller unresolved margins remain inconclusive.

See synthesized-plan.md for the complete proposed protocol. Its conclusions can be
"adequate for one qualification" or "inconclusive; stop", never "keep tuning".

## Claude's synthesis review and accepted corrections

Claude explicitly judged the synthesis stronger than either independent proposal.
It conceded the linked rotations, pooled-bias problem and acknowledgement bypass,
and accepted retained objects, the shared-fixture contrast, fully timed 64-call
blocks and a controls-only method decision. Its preference for rebuilding Q objects
is retained as a disagreement; we keep retained instances and uncertainty screening.

Its strongest correction was feasibility: the draft combined 72 eligible all-pass
control observations with only 200 ms accumulation. A planning calculation using old
within-cell variation suggests that sampling error could frequently trigger the guard
before meaningful identity comparisons. The model's independent-normal assumptions
and extrapolation to the new sampler are unverified, so its very small projected
pass probability is not treated as fact. It still identifies a real design risk.

The final plan therefore adopts:

- 800 ms accumulation for the original AND 64 KiB workload and every matching control,
 including the closed anonymous conditions; 200 ms for other original workloads;
- Removal of separate-data swapped controls, reducing Phase R to 96 configurations,
 while preserving 72 eligible observations and all original 1 µs component guards;
- The same sample-round interleaving style in R and Q;
- Phase-specific configuration namespaces and an explicit eight-object Q construction
 list, with closed-snapshot compatibility checked before timing;
- An explicit 146-cycle/sample bound and 16,384-record reservation, with honest headroom
 estimates rather than a claimed duration-derived hard bound;
- 45 minute timing caps for each phase. Minimum measured time is 35.84 minutes per phase;
 roughly 40 minutes is a planning estimate, not a promised completion time.

This spends more sampling time prospectively without changing a performance budget.
No duration will be increased after results. The stricter null-envelope completion
screen and revised caps are explicit parts of the proposed plan for later execution
authorization. The current task remains design only.

Claude's final reconciliation accepts the corrected plan as ready for implementation
review. The final plan also discloses candidate/closed macro-schedule asymmetry and
includes seven untimed prefix records in the capacity count. The coordinator keeps
the 45 minute cap: its modest headroom is deliberate, not a promise of completion.
Call slowdown does not translate proportionally to phase time because the sampler
stops after an accumulated-time target. None of the planning projections establishes
a success probability or changes a performance gate.
