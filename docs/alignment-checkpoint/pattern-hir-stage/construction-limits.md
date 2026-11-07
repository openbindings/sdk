# Logical construction limits and host stack evidence

Current direction, independently reviewed against the pinned specification: use
hard checked logical recursion limits before uncooperative dependency stages;
qualify physical stack for recorded builds/artifacts. Do not promise that every
Rust compiler and optimization profile has identical stack frames. No runtime
Rust-version allowlist is introduced, and the old stack_bytes-derived-depth API
has no compatibility protection.

The pinned core's schema meaning and resource-attack guidance do not mandate a
byte-exact stack guarantee for arbitrary Rust source builds. Rust>=1.88 remains a
language/build support declaration. Source-build callers must provide adequate
host stack, just as the existing support guidance already distinguishes debug
from release footprints. Shipped Wasm artifacts have concrete link/host identities
and can carry their corresponding exact stack qualification.

Before Reader recursion, enforce the supported authored-group ceiling of 32,
allowing a caller to tighten it. The normalized closed grammar allows checked D+1
groups because boundary/empty-set lowering adds at most one atom-local group.
Before HIR translation, enforce bounded prospective simultaneous prefix calls and
structural equality height. Before NFA construction, enforce a bounded actual HIR
compiler path, including its helper/call structure; repetition expansion remains
sequential work/allocation rather than additional physical recursion. These are
hard logical ceilings, independent of a caller-supplied byte budget. A source/group
limit can become stricter but cannot bypass the construction ceilings. Each stage
checks before entry and never publishes after cancellation or arithmetic failure.

The independently reviewed HIR limits are 64 prefix calls and 256 equality levels,
with frozen boundary/control adjudication. These are implementation capability
limits, not new specification concepts or public HIR/profile APIs. Any change must
preserve required current-model outcomes and get source/workload requalification;
refusal never counts as a required semantic success.

The earlier construction-stack-policy receipt proposed a universal 256 KiB entry
profile split into a 192 KiB recursion window and 64 KiB residual. Keep that receipt,
its emitted weights and observed failures unchanged as historical/source evidence.
The public source-SDK promise above supersedes that proposal's universal reading.
Use concrete native/current/minimum/debug/LTO and linked Wasm measurements for
actual supported-build claims; report their limits without extrapolating weights
across targets or compiler revisions.

This decision does not close whole-compiler qualification. Complete resource
accounting, admitted default workloads, actual native/host checks, matched
performance, and final original-evaluator integration remain required. Removal of
the outgoing compiler also removes its stack allocation heuristics, diagnostics
and public options; original-engine proof depth receives explicit logical meaning
instead of inheriting a misleading byte conversion.
