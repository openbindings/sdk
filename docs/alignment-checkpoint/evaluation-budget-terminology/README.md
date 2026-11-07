# Evaluation allowance terminology

The unpublished Rust/serde API now exposes `evaluation_sdk_work_units` and
`evaluation_sdk_reservation_bytes`; TypeScript exposes `evaluationSdkWorkUnits`
and `evaluationSdkReservationBytes`. Retired spellings are rejected, including
mixed old/current objects. No public aliases or compatibility layer remain.
Types, numeric domains, defaults, quotas, charge sites and diagnostic codes are
unchanged. Work defaults to no additional caller cap; reservation defaults to
48 MiB.

These names describe cooperative SDK work and cumulative SDK storage estimates,
including proof, memo, scratch, search-cache and diagnostic storage. They do not
claim instruction counts, callback frequency, exact allocator requests, peak
heap, immutable pattern-program size or a process-memory ceiling. Each invocation
starts fresh; verdict and diagnostic passes share its remaining allowance.
Required custom-meta evaluation debits both its own evaluation allowance and the
enclosing preparation Attempt. Either exhaustion can therefore cause a preparation
Limit. Current guides, declarations, tests, private fixtures, probes and source
bindings use the replacement names.

The private measurement request carrier stays frozen with its historical field
names. Only its SDK adapter moves raw values into current SDK fields. Replacement
SDK spellings are rejected even when supplied alone; omission/null/zero and native
usize behavior are preserved. No workload, numeric limit, oracle or measurement
phase changes. Two existing nested location guards were written as equivalent
short-circuit let chains to satisfy warnings-denied tool lint; their mapping and
order are unchanged. Historical tables, pinned inputs and earlier receipts retain
their original spellings and identities.

Independent review passes 27 targeted tests on each of Rust 1.99 and 1.88, covering
defaults/serde, retired-key rejection, native domains, actual selected-meta limits,
fresh calls, retries, cumulative Explain and sticky control causes. The final
release adapter is compared through all 42 frozen fixtures and 101 mode rows with
complete observations; its own source and binary identities are retained.

Parent validation passes default-feature workspace tests (1,013 passed, no
failures, six ignored), workspace lint, all 1,546 required evaluator values,
fixture/input integrity, adapter tests and tool lint. Rebuilt Wasm, TypeScript
types/reference checks, Node 189/189, four Rust consumer configurations and installed
TypeScript package/README/lifecycle examples pass. The first Node run was denied
local listeners; the authorized rerun passes. An initial package run could not
write the shared npm cache; a separate writable task cache resolves it. These
environment attempts remain visible in the parent qualification receipt.

Copied evidence excludes executable binaries, bulk copied SDK trees and the large
source patch already represented by this Git commit. Historical commands refer
to the coordinator layout. Current source-map consistency is a distinct source
check and makes no runtime claim. Final whole-source/native-feature host,
browser/package/consumer qualification, full original/candidate performance
acceptance and the G1–G7 completion audit remain open. This feature checkpoint
authorizes no merge or release.
