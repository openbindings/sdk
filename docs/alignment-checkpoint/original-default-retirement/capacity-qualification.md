# Default-engine capacity and host qualification

The original large opaque-resource and constant-value workload is restored.
Fixed standard meta checks reserve for the names and values they actually read;
unknown payloads retain their name and token cost without charging a traversal
that never happens. The complete fixed graph guards the 46/59/61 known-name sets
across the three supported editions. Custom meta-schema checks are unchanged.

Const/enum scalar predicates compare exact values without retaining full scalar
identities. Strings stream exact UTF-16 units with cancellation checkpoints;
numbers keep exact canonical decimal equality and existing parse reservations.
Composite, uniqueItems and semantic-effect identity handling remains separate.
[Independent capacity review](capacity-review.md) passes on both Rust versions,
including the unchanged two-17-MiB-resource and greater-than-32-MiB-constant job.

The bare-empty-alternation bound now counts one Alternation plus its Empty children
instead of assuming concatenation overhead. The 16,384-node ceiling is unchanged.
The preserved 10,000-alternative workload passes, 16,382 separators fit, and 16,383
are refused before parser entry. [Independent review](capacity-empty-alternatives-review.md)
passes; all 57 structural compiler groups pass on both Rust versions.

The full current native workspace phase passed 1,050 checks with six existing
release-only ignores. The minimum-Rust workspace passed its code checks; its
13 local-server permission failures were then rerun in the authorized HTTP suite,
where all 16 checks passed. These runs preceded the separate narrow alternation
correction, which has its own current/minimum replay. Do not add overlapping counts.

The actual rebuilt Wasm package passes all 187 Node tests and both browser judges:
505 core cases and 1,546 required evaluator cases per Chromium/WebKit host. The
browser example now displays the original sourceUri and fixed assertion guidance.
Rust's three release stack groups pass on the recorded macOS build. Type checks,
formatting and the 15 external consumer feature selections pass. The standalone
consumer lockfiles now omit the evaluator's retired production regex dependency;
regex remains a Reader test oracle and an indirect fixed-check dependency.

Old projection/stack/slot/lazy-compilation assertions were replaced with the actual
policy boundaries while retaining workload schemas and values. TypeScript's fixed
preparation defaults can refuse the 8,000-resource request on shared work before
its resource-count cap; this is explicitly recorded rather than presented as a
resource-count-only measurement. Native tests separately fund the preparation
budget to isolate that count policy.

The complete portable verifier reached consumer qualification after the successful
workspace phase; its stale consumer lockfile was corrected and that feature matrix
now passes. Final full-verifier continuation, final hosted CI including Windows,
archive/reference checks, matched performance and the final alignment audit remain
open. This is a recovery checkpoint, not a completed-SDK claim.

[Source and receipt index](capacity-checkpoint.json) records this increment.
