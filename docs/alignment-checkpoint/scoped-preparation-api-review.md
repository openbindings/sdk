# Scoped preparation API: bounded review

PASS for commit f94f73bac23d40ed916d2e45a46ca834dcfd96ce. This review covers
the33-file scope/terminology/API migration, not the separately dirty inactive
AST/HIR/NFA implementation. Exact commit contents and hashes are retained here.

The two Rust PreparationLimits fields now say max_sdk_work_units and
max_sdk_reservation_bytes, with corresponding preparation-sdk-* limit codes.
Defaults, Attempt nesting/poisoning/control ordering and actual charge sites remain
unchanged in this commit. Core environment/registration and evaluator entry points
forward the renamed values consistently. No compatibility alias remains.

The public docs explicitly distinguish cooperative SDK work and explicit storage
reservations from arbitrary dependency operations, allocator requests, callbacks,
deadlines or process memory. Generic evaluators must declare their own resource
scope. The replacement pattern compiler is still described as inactive/in progress;
this contract change is not claimed to complete its construction qualification.

Updated tests retain their numeric and typed-failure obligations; only renamed
field/code expectations and formatting changed here. Pinned semantic verdicts,
original location expectations, no-I/O and outcome boundaries are unchanged.
TypeScript/Wasm did not expose these PreparationLimits fields, so no new host
options or undocumented aliases were added. Comparison/source-pull budgets were
not changed. Active-source search found the old names only in the separately
scoped comparison API and preserved historical receipts, as intended.

Parent evidence is current/minimum287 focused tests, workspace check/strict lint
and docs. This bounded independent result is a source/diff/disposition review; it
does not claim an independently repeated whole-workspace build. Existing separate
Attempt resource/cancellation tests and the current owner probes retain their
prior algorithmic meaning.
