# Bounded original pattern search cache

The former state-only reservation missed wide epsilon closures: four NFA states
can have thousands of outgoing epsilon branches. The replacement derives backing
from actual sparse arrays, slot tables and maximum pending branches, including
cumulative Vec growth and per-vector platform limits. Graph reservation addition
also rejects integer overflow instead of saturating into an unlimited allowance.
It does not move VM compilation or certify that separate compiler schedule.

Independent design review approved the bounded correction. Parent production
checks pass: five resource/matching groups, 926 current Rust checks (six ignored),
320 minimum-version library checks (two ignored), strict lint, 184 Node tests,
types/format and 352 installed-package assertions. Chromium and WebKit each pass
505 core and 1,546 required evaluator cases, five cache regressions and the prior
syntax/frame/reference host groups. Independent implementation/resource review passes with no unresolved finding:
1,546 required cases, 62 inherited controls in both modes, ten actual-Graph
allocation probes and 23 private checks. The production checkpoint is pushed as
`421706c`. No whole-SDK completion or default-switch claim is made.

Ten native allocation probes import the actual production cache module with a
recording-only ledger; the unit tests exercise the actual Graph's limits and
cancellation. For 1,000 empty alternatives, reservation is 96,320 bytes and actual
cumulative requested backing is 16,192; for 10,000, 960,320 and 160,192 respectively.
All ten cache owners return to zero on drop. These are cumulative requested bytes,
not RSS or the dependency's retained-length memory_usage estimate. The prior
design-formula-only probe is retained separately and is not production evidence.

Exact commands, source/artifact identities and log digests are in results.json,
source-hashes.json and probe-command.json. The first attempted Wasm build command
had an invalid log destination and did not execute; the corrected command rebuilt
successfully before all host runs. No assertion or expected result was weakened.
