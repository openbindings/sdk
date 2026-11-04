# Allocation-free Unicode identifier membership

The independent pattern audit found unaccounted cold construction of two global
regex engines while checking non-ASCII capture-group names. A short `(?<é>x)`
reader call requests 798,632 bytes against a 1,536-byte reader reservation; using
two non-ASCII characters initializes both engines and requests 1,617,239. Warm
behavior hides this defect. These globals have no compatibility protection.

Replace them with private static inclusive scalar ranges generated from the exact
locked regex-syntax 0.8.11 public HIR API for ID_Start and ID_Continue (Unicode
16.0.0). Independent exhaustive comparison already establishes equality with the
existing anchored matcher on all 1,112,064 scalar values for each property.
Use binary search over sorted nonoverlapping const ranges, with no parser, heap,
global mutable state or initialization on the runtime path. Current group-name
grammar still owns its explicit dollar/underscore/join-control additions, escape
decoding, duplicate detection and invalid/lone-unit rejection. Pattern language
classification is unchanged; this removes an unnecessary runtime implementation.

Keep the generated tables in the private pattern identifier module. Preserve the
upstream Unicode data license beside the generated source and identify the pinned
generator dependency and Unicode version in its header. A small offline developer
generator/checker under tools reproduces the exact table file from the public API;
its explicit check mode and dependency identity fail on drift. Production builds
never regenerate source automatically. The existing attribution inventories retain
the dependency's Unicode license; the copied data also carries its own source
notice. No new public API or normative concept is introduced.

Validate exact generated range equality, sorted/disjoint endpoints and adjacent
scalars, all-scalar comparison, ASCII and explicit grammar additions, combining
marks/Other_ID, astral names, raw/escaped names and collisions, invalid starts and
lone surrogates. Meter actual cold and warm reader calls, including concurrent
first use, repeated distinct groups and cancellation under the existing caller
budget. Document the maximum membership-search comparisons from the actual table
lengths. This certifies that lookup component only; the existing width-quadratic
duplicate-group-name scan and the rest of reader accounting remain separate audit
obligations. Unicode 16.0.0 is the existing locked data, not a new ECMA property
alias policy.
Retain current pattern and forced-original suites and actual Wasm host checks.
The separate AST/HIR/compiler construction certificate remains open.
