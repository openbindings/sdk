# Prepared numeric and count readings

The bounded optional operand cache passes implementation and independent review
against SDK `4998c72`, at the four identities in `source-hashes.json`. Tokens up to
256 bytes are prepared within a 65,536-byte source and 1,024-node-record policy.
Oversized or excess operands keep the charged original-reading path. Invalid
local operands remain local readings. The cache borrows exact numeric bounds;
it does not erase schema-dependent comparison work or allocation charges.

`results.json` records native current/MSRV, strict lint, required-value, actual
Wasm/Node/types/package and Chromium/WebKit results. Logs and host judgments are
retained alongside it. The independent review is in
`../original-engine-numeric-review/implementation/review.md`; it exercises all
1,546 required cases through the original engine, unchanged design controls,
large frozen workloads, boundary accounting, cancellation, ownership and reuse.

This is a bounded preparation improvement, not the default-engine switch or a
whole-SDK completion claim. Full pattern preparation, local syntax accounting,
iterative execution, original diagnostic witnesses, outgoing compiler removal,
final matched performance, all-host qualification and final map audit remain.
The simultaneous CI/package repairs are separate and are not covered by this
numeric implementation review. No old API compatibility is required.
