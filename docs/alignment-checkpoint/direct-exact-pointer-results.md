# Direct exact pointer construction

The internal exact-pointer writer now measures and fills its final UTF-16 buffer
directly when a relative path contains an unpaired unit. It allocates neither an
ancestor/decoded-name vector nor speculative scalar text. Scalar paths retain
their direct writer, including retained subtrees below an excluded unpaired
ancestor. The new constructor is private; no public API was preserved or added
for compatibility.

The unchanged independent allocation witness drops from 323/1,547/3,179/6,443
cumulative requested bytes to exactly 18/66/130/258 at depths 8/32/64/128; all
storage releases on drop. Independent implementation review passes 16,506 checks
over 236 exact documents and 4,327 retained subtree roots, including escaped,
astral, lone-unit, empty-name/index and serialization controls.

Parent current/minimum Rust exact-JSON suites pass 65 checks each; the current
workspace passes 928 checks with six release-only exclusions. Strict lint, 185
Node tests, types/format and 352 installed-package assertions pass. Both browsers
pass 505 core/1,546 evaluator cases plus four direct-pointer cases and the existing
pattern/syntax/frame/reference controls. Exact source/artifact hashes and commands
are preserved beside this report. No expectation was weakened.

This is the pointer construction foundation for original Explain. It does not
implement witness propagation or the diagnostic materializer's caller budget and
ancestry checks. Full pattern construction, default-switch/retirement and final
performance/host/alignment audits remain open.
