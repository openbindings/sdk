# Shared completed standard-syntax facts

Independent design review passed before implementation. One temporary Construction
owns the immutable SchemaEnvironment and borrows its single preparation attempt.
A dense Vec<Cell<usize>> records only successful local standard syntax checks;
Plan::at, activated preparation and complete meta syntax traversal use that same
object. Completed Plans retain neither the cells nor the attempt. The cells allow
immutable space borrows during synchronous construction without a second environment
or caller-supplied profile argument.

Each lookup derives the actual original node/profile in that space and charges
fixed work. Only a completed true result followed by the shared attempt post-check
publishes a bit. Unknown/custom profiles cannot consult or populate positive bits.
Dense node_number indices include packaged nodes without using their top-bit IDs
as vector offsets. Full requested backing and initialization are charged before
try_reserve_exact; arithmetic/allocation failure marks the same sticky attempt.

A hit skips only dialect_validity_bounded. Resource eligibility, profile/depth
checks, reference and dynamic handling, unused-child traversal and custom-meta
obligations still run. A syntax-valid false schema still rejects a consumer. A
cached helper cannot hide invalid unused definitions, contentSchema, orphan
conditionals, nested custom consumers or an admission cycle. Existing internal
signatures were changed freely; no compatibility adapter was introduced.

Parent validation passes 548 current Rust checks (six separately identified
release-only checks ignored), 195 minimum-Rust library checks (two release-only
checks ignored), and strict Clippy. The seven focused groups include profile/source
identity, packaged indexing, actual hit work removal, exact work/backing boundaries,
ignored nested exhaustion, publication cancellation and fresh owners. The review's
initial Draft-07 items:[] witness was invalid itself; the corrected identical
items:[true] child under properties separates Draft-07 from 2020-12 without changing
any production behavior or semantic expectation.

Fresh Wasm passes 183 Node tests, types, 352 installed-package assertions and
format checks. Chromium and WebKit each pass 505 core cases, 1546 required values,
eight frame cases, four long-reference cases and seven new shared-syntax cases.
Final rebuilding after the equivalent arithmetic spelling and an unrelated
invocation documentation correction produced byte-identical Wasm/JS/signature
artifacts. Exact source/artifact identities and logs are in results.json.

Independent implementation/resource qualification is in progress. Its first replay
passes all 1546 forced-original cases and 62 inherited controls; storage, actual
checker counts, publication boundaries and cleanup still require its final report.
No whole-SDK, pattern-compiler, original Explain, final default-switch or final
performance claim follows from this checkpoint.
