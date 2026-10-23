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
above each call's baseline, not RSS, allocator reservations or Wasm memory. A
reallocation counts as holding its old block and its new one at once, whether or
not it moved, so peaks are conservative.

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

## Pattern forms

The evaluator writes each pattern in a form for its linear-time engine that
spells every set out, so a 10,002-byte pattern of 5,000 `\S` has a 955,002-byte
form. The pattern phase (`src/patterns.rs`) prepares contracts holding such
patterns: 380 of them, as many as `Limits::total_pattern_bytes` admits, one
more, one repeated 380 times, and one more than it admits as
`patternProperties` names. Each preparation, and checking `{}` when it
prepares, must add no more heap at its peak than preparing the same document
with the patterns as annotations, plus the default total and three times the
per-pattern limit for the form being written and its shared copy. Run the
phase alone with `--patterns-only`.

## Compiled patterns

The same phase bounds the memory the regex crate builds for a contract's
compiled patterns, which share `Limits::total_pattern_memory` (64 MiB). It
prepares 40 and 300 `patternProperties` names of the form `^.{1,10000}N$`, each
compiling to about 13.2 MiB, which need more together than the memory leaves
and must refuse with `pattern-total-memory-limit` in under 2 s (each needs more
than its equal part, which takes about 0.2 s to establish), adding no more at
their peak than three times the memory divided among them to the same
document with the names as property names.
Four such names must prepare and decide a value, keeping no more than the
pattern memory and peaking within twice it. One hundred names whose automata
fill the lazy DFA's cache (`[01]*1[01]{20}zN`), checked against a 64 KiB key of
random bits, must add no more than the pattern memory at their peak.

## Default admission limits

The limits phase (`src/limits.rs`) validates 34 documents at the default
admission limits (32 MiB, 1,000,000 nodes, any nesting), most filling the byte
or node limit, with structures document validation builds: a million nested
arrays around a long
string; half a million invalid operation names, alone and beside a string
filling the byte limit; a million escaped strings; one escaped string and one
string with an unpaired surrogate escape filling the limit; a million-element
`allOf`, a million aliases, a half-million-member object, half a million
schemas and a quarter million absolute `$id`s, each beside an escaped string
filling the remaining bytes; half a million invalid operation names holding
escapes, filling the limit; a third of a million `$id`s that each add
directories, under a base or with authorities of their own, alone, in pairs and
with a query and fragment, each beside an escaped string filling the remaining
bytes; a third of a million escaped `$id`s, each with its own authority and a
dot segment that makes its path a copy, alone and with a query and fragment;
an operation name, an extension name and an alias of U+E000, the
character fixed checks escape beside encoded unpaired units, filling the limit,
and the operation name beside an unpaired unit; 166,000 nested relative `$id`s,
and 70,000 of them
each declaring a repeated `$id`; one `$id` of 4 million segments (8 MiB) and of
16.8 million (32 MiB), each alone and beside two identifiers declared twice; one
`$id` interrupted by dot segments throughout; a 28 MiB authority read back by
150,000 relative `$id`s; 1,749 relative `$id`s, each giving up a different
number of a long `$id`'s 5 million segments; and a malformed reference at every
level of a 120,000-level schema chain. Each `validate_document` call must add at
most **300 MiB** of live allocation above the caller's input, and must reach its
expected conclusion. The largest, a third of a million escaped `$id`s with
copied paths, a query and a fragment, adds 256.0 MiB, 44.0 MiB below that. The
ceiling is not a multiple of the byte limit: at the former 64 MiB default the
same shape adds 364.9 MiB, since much of its memory is held for each
declaration, whatever its length. This bounds these shapes, not every document.
Run the phase alone with `--limits-only`.

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
Ordinary and referenced failures use an `anyOf` whose second branch fails at
every element, directly and through a `$ref`, so building the library's errors
would allocate for each element; the work-capped case checks an integer array schema under a
1,000-unit evaluation cap, which reading the elements exhausts, and retains its
expected no-verdict. Each call must use
at most **65,536 bytes** of incremental live allocation. This is a focused
regression threshold for prepared type decisions, not a universal value bound.

Run `python3 tools/diagnostic-memory-probe/check_value_mutations.py` to test the
allocation controls. It copies sources under `target/diagnostics-review`, replaces
the cheap check's `is_valid` call with `validate(...).is_ok()`, runs the same
assertions for each failing path alone, and restores the copy. It never mutates
the production evaluator. Each path must fail the allocation assertion while
retaining the expected public outcome. Fresh mutation receipts are generated under
`target/diagnostics-review/value-mutations`.
