# Current interface packaging and CI repairs

The numeric implementation is saved at `b380d61`; the current interface CI repairs
are saved at `6988e6d`. Bounded independent source review passes after fixing the
missing-ledger finding. The clean package candidate at `6988e6d` and the complete
local verification command pass. A followup `.gitattributes` correction addresses
the hosted Windows checkout failure; actual rerun qualification is pending.

## Repairs and independent evidence

The fixture checker accepts both current table formats while requiring all six
current interface ledgers and discovering any additional ledgers. Every file must
have a matching SHA-256 row. Independent review reproduces the former missing-
ledger bypass and verifies its repair, all 32 files, 21 exact published-input
matches, and 23 positive/negative integrity controls. No byte normalization or
missing-fixture waiver is used.

Four current companion archives now carry LICENSE and NOTICE; obsolete core pins
in existing notices are corrected without deleting attribution. The standalone
benchmark lock adds 24 registry identities already in the root lock. Nine
observation tests and 48 comparison-policy tests pass. The missing-reference
fixture remains an Undefined control, while preparation-refusal counting uses a
real explicit resource refusal. No performance threshold or comparison policy is
weakened. Final matched performance remains open.

## Completed validation

`python3 tools/verify.py` passes on the current Rust toolchain, including 888
workspace tests/doctests (four existing release-only checks ignored in that
lane), strict lint, downstream consumer/dependency combinations, attribution,
references/snippets, source-pull/invocation/resource/diagnostic checks, release
build and stack checks, and the full 505-case core corpus. Full output is retained
in `full-verify.log`. Repeated test and corpus inventories are not additive
semantic coverage. The first run lacked local-server permission; its exact
failure log is retained alongside the permitted passing run.

Clean Wasm/TypeScript rebuild and `package-candidate.py --offline` pass at
`6988e6d252ea224a77b464c9ed83e6d28be3f22f`. The manifest records thirteen Cargo
archives, the npm archive, extracted-archive consumers, attribution and build
identity. No publication was performed. Exact identities are preserved in
`package-manifest.json` and the compact `results.json`.

## Windows checkout correction

Hosted Windows CI failed because `.gitattributes` still named the deleted
inspection/synthesis crates and omitted four current companion fixture trees.
The correction removes the obsolete paths and marks the current fixture trees
as exact bytes. A Git checkout-filter reproduction with `core.autocrlf=true`
changes all 28 affected files before the fix and preserves all 28 after it.
`windows-checkout-before.json`, `windows-checkout-after.json`, and the hosted
failure log retain the evidence. The digest verifier still passes 32 files and
still rejects actual CRLF byte drift. A real hosted rerun remains required.

Whole-SDK engine, final matched performance, all-host CI and final map gates stay
open. No old API or incidental signature/ownership behavior is protected.
