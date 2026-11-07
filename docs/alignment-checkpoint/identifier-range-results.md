# Static identifier property ranges

Unicode group-name membership now uses generated const ranges from the exact
regex-syntax 0.8.11 public property API and its existing Unicode 16.0.0 data.
This removes two unaccounted cold regex engines and their retained global storage.
Cold/warm reads of `(?<é>x)` now request 184/184 bytes instead of 798,632 on first
use; `(?<éé>x)` requests 186/186 instead of 1,617,239. All read storage releases.
Grammar additions and decoded name identity remain unchanged.

Independent review passes 2,224,128 scalar comparisons, 16 private tests, 800
concurrent reads, eight generator controls, exact license/lock identity and
1,546 required cases plus 62 inherited controls in both modes. Its Windows
checkout finding is resolved with a byte-preserving rule; the checker remains
exact. No increment finding remains open.

Parent gates pass 315 current evaluator/support checks (five release-only
exclusions), 106 minimum-Rust library checks (two exclusions), generator
reproduction on both toolchains, strict SDK/generator lint, attribution, 185 Node
tests, types/format and 352 installed-package assertions. Chromium and WebKit
each pass 505 core/1,546 evaluator cases, nine pattern/identifier cases and the
prior pointer/syntax/frame/reference controls. Source/artifact hashes, exact
commands, initial harness errors and final logs are retained beside this report.

The portable verifier now requires exact generated-table reproduction. Production
builds never regenerate source. This is a bounded membership correction, not a
certificate for the rest of the reader or VM compiler. The duplicate-name width
cost, full compiler schedule, original Explain, default switch/retirement and
final performance/host/map qualification remain open.
