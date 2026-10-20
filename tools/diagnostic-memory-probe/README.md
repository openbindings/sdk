# Document findings allocation qualification

Run `python3 tools/verify-diagnostics.py` with the offline toolchain environment.
The standalone crate isolates allocator instrumentation from the safe SDK
workspace. The generator produces 67 exact 1 MiB documents,
adds two finding-count/pointer-retention controls, and selects 305 corpus document
inputs. Of those corpus inputs, 300 parse; the five parser refusals remain recorded.
Every accepted input has three samples. No vendored source is modified.

The probe separately measures:

- Cold location-map compilation, including the library's initially populated caches.
- Library validity alone for OBI-02 and the localized OBI-10 nodes.
- The production OBI-02 location walk on established failures, with original source
  coordinate materialization. Its callback retains positions under the same count
  and pointer allowances; collector bookkeeping is included in the next measure.
- Fixed checks with findings and original locations, each check dropped before the
  next while retaining admitted source positions.
- Complete public document validation, using a fresh parsed owner for every sample
  so cached validation cannot replace work.

Parsing, the isolated-check schema index, warmed compilation and receipt encoding
are outside the measured regions. Complete validation includes its native checks,
index/proof/report construction. Numbers are incremental live System allocation
above each call's baseline, not RSS, allocator reservations or Wasm memory.

## Regression bounds and reason

On this frozen input suite, validity must stay at or below **18 MiB**. Fixed checks
with guidance must stay below that input's measured validity peak **plus 10 MiB**;
complete document validation must stay below validity **plus 12 MiB**. These are
regression thresholds for these inputs and this implementation, not universal
bounds for arbitrary documents or value explanations.

The location walk keeps depth frames and one child candidate list, never a library
error tree or a vector of all instance children. Its schema plan is fixed at
compilation. Retained findings are capped at 4,096 and aggregate source pointers
at 8 MiB. The 10 MiB allowance leaves 2 MiB for traversal/collection/name scratch
on this suite; the complete SDK allowance adds 2 MiB for native checks and reports.
The separately capped validity baseline also detects increases in the fixed schemas' validity allocation cost.

The verifier rejects diagnostic API calls in the two fixed-check modules. It also
requires exact agreement between validity-only and fixed-check verdicts/refusals
for every sample.

## Near misses

The fixed checks have no work budget: their patterns run on the library's
finite-automaton engine. The near-miss phase (`src/near_misses.rs`) validates
near misses of the SemVer pattern (dotted and plain pre-release shapes), the name
grammar (on an alias) and the meta-schemas' anchor and identifier patterns at
1 MiB and 8 MiB, after compiling the checks and warming the matcher caches. For
each shape, validity alone and the check with findings must use no more peak
bytes and no more allocations at 8 MiB than at 1 MiB. A backtracking matcher
keeps a saved position per character it may retreat over and fails this; the
finite automaton allocates the same few hundred bytes at both sizes. Complete
document validation is recorded beside them but not bounded here: its native
rules and finding locations are outside the fixed checks. Run the phase alone
with `--near-misses-only`.

## Recorded results

The verifier writes fresh raw allocation and timing samples to
`target/document-diagnostics/memory.jsonl`; `summarize.py` writes
`target/document-diagnostics/allocation-samples.json`. Historical samples and
migration receipts are archived outside the product. The current generators and
allocation assertions remain the acceptance contract.

## Independent container checks and default values

Container projections neutralize the six descending applicators' child constraints
while retaining their selectors and evaluated-position coverage. All other
constraints, including `required`, `uniqueItems`, size constraints and `anyOf`, run
on the original container through `is_valid`. Local direct references point to
container projections; references inside non-descending constructs still resolve
to the original schemas. Projections are added under a fresh definition in the
same pinned root, leaving that root's assertions intact. A container failure is
retained even if children fail. The probe includes the additional cached
compilation cost; per-document allocation thresholds remain unchanged.

The allocation runner now also prepares default value contracts outside the
measured region and validates arrays of 1,000 and 50,000 elements three times each.
Ordinary and partial-upper failures use a two-branch type `anyOf`; a missing
resource forces the partial programs. The partial-lower case reaches both Boolean
passes and retains its expected resource-unavailable outcome. Each call must use
at most **65,536 bytes** of incremental live allocation. This is a focused
regression threshold for prepared type decisions, not a universal value bound.

Run `python3 tools/diagnostic-memory-probe/check_value_mutations.py` to test the
allocation controls. It copies sources under `target/diagnostics-review`, replaces
one `is_valid` call at a time with `validate(...).is_ok()`, runs the same assertions,
and restores the copy. It never mutates the production evaluator. Each mutation
must fail the allocation assertion while retaining the expected public outcome. Fresh mutation receipts are generated under
`target/diagnostics-review/value-mutations`.
