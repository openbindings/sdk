# Document findings allocation qualification

Run `python3 tools/verify-diagnostics.py` with the offline toolchain environment.
The standalone crate isolates allocator instrumentation from the safe SDK
workspace. The generator reproduces the investigation's 67 exact 1 MiB documents,
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
bounds for arbitrary documents or upstream value diagnostics.

The location walk keeps depth frames and one child candidate list, never a library
error tree or a vector of all instance children. Its schema plan is fixed at
compilation. Retained findings are capped at 4,096 and aggregate source pointers
at 8 MiB. The 10 MiB allowance leaves 2 MiB for traversal/collection/name scratch
on this suite; the complete SDK allowance adds 2 MiB for native checks and reports.
The largest observed overheads were 8,472,586 bytes and 8,797,823 bytes respectively,
from the long-ancestor retention control. The separately capped validity baseline
also detects increases in the fixed schemas' validity allocation cost.

The verifier rejects diagnostic API calls in the two fixed-check modules. It also
requires exact agreement between validity-only and fixed-check verdicts/refusals
for every sample. A location budget stop retains failure and limited guidance.

## Recorded results

[Raw numbers, input hashes, source/schema hashes and summaries](../qualification/inputs/document-allocation-samples.json)
come from release macOS arm64 runs. The generated full receipts are in
`target/document-diagnostics/memory.jsonl`; `summarize.py` retains their raw numeric
samples in `target/document-diagnostics/allocation-samples.json`.

Cold map compilation: **3.351 ms, 1,116,066 bytes peak**. Subsequent isolated rebuilds
used 569,351 bytes and 0.661/0.686 ms; these are not additional per-document costs.

| Input | Validity peak | Fixed checks + locations | Complete validation |
| --- | ---: | ---: | ---: |
| 1 MiB document schema union | 928 B | 1,024 B | 2,442 B |
| 1 MiB meta type union | 4,194,296 B | 4,194,305 B | 4,194,965 B |
| 1 MiB meta dependencies union | 4,194,240 B | 4,194,249 B | 4,194,909 B |
| Maximum of 69 large inputs | 17,573,888 B | 17,574,216 B | 17,949,677 B |
| Maximum of 300 corpus documents | 1,120 B | 1,024 B | 3,930 B |

The largest large-input peak is uniqueness validity, not diagnostic construction.
For the 95 corpus documents that enter the OBI-02 location walk, its median time
was **3.041 microseconds**, peak **1,024 bytes**. Across all 300 corpus documents,
fixed checks including validity and findings had median **2.563 microseconds**;
complete validation had median **4.541 microseconds**. Per-case raw timings are
retained; independent timing samples must not be subtracted as an exact cost.

## Locations and migration

The earlier claim of 19 enclosing-array cases was incorrect. Its raw finding
ledger has **five array cases and fourteen OBI-10 checked-node cases**. All five
arrays now locate their failing elements; the fourteen meta cases deliberately
retain checked-node locations. All nineteen IDs and expected pointers are in
[document-location-expectations.json](../qualification/inputs/document-location-expectations.json)
and checked by `check-diagnostics-migrations.py`.

The [diagnostics migration ledger](../qualification/inputs/diagnostics-migration-ledger.json)
lists each changed kit/corpus case and rejects verdict, refusal and evidence drift.
The [verification receipt](../qualification/inputs/diagnostics-verification.json)
lists commands, passing lanes and loopback denials. The inert property-name
admission hook and the unused diagnostic patch paths are documented precisely in
[DEPENDENCY-MAINTENANCE.md](../../DEPENDENCY-MAINTENANCE.md).
