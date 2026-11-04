# Original Explain implementation progress

This is an unfinished feature-branch increment against the pinned specification
c1419c9 and interfaces 5815fb1. It implements the approved compact-witness design,
not the default-engine switch or whole-SDK completion. The prepared original-node
interpreter now records concrete false witnesses only in an optional second pass;
both passes consume one cumulative allowance. Exact source and instance pointers
are materialized only for the final selected witness, with finite SDK messages.

The independent 42-control fixture is copied byte-for-byte into the SDK. Parent
native and public Wasm tests preserve every input, expected verdict, forbidden
branch, exact pointer and source URI. Seven native test groups cover these cases,
exact/one-below cumulative work and bytes, every cancellation checkpoint, enclosing
sticky Attempt exhaustion, zero/unlimited/overflow limits, primitive evaluation
order and 200 concurrent successful calls paired with cancelled attempts.

The first current-Rust workspace run without loopback access failed only at local
HTTP server creation. A permitted loopback run exposed the old recursive Explain
cancellation expectation described below. After adjudication, the complete
workspace, minimum-Rust library tests, strict lint, 186 Node tests, 352 installed
package assertions and both actual browser lanes passed. Each browser also passed
505 core cases and 1,546 required evaluator values, plus the new 42 original Explain
cases and the prior frame/reference/syntax/pattern/pointer controls. These passes
precede R-EW2's final message-allocation correction; final reruns remain pending.

## Independently reviewed adjudications

The recursive cancellation fixture previously required NoVerdict at every observed
cancellation. That was obsolete once Explain gained its approved verdict-first
second pass. The same original cases and exhaustive cancellation/retry checkpoints
remain. The test now independently measures the ordinary verdict boundary and
requires Fails with empty/incomplete details for post-verdict Explain cancellation.
Pre-verdict cancellation still requires NoVerdict. S/U/N receive no exception.
The independent reviewer approved this exact distinction.

The frozen explain-one/explain-many workload inputs remain unchanged. Their old
first problem message is `expected JSON type: integer`; the new finite message is
`value fails this schema assertion`. The reviewer approved this wording-only
adjudication. Fails, type code, /0 instance, original /items/type source URI, one
problem, incomplete diagnostics and resource/performance gates remain mandatory.
Historical wording is retained in the original fixture, not silently rewritten.

The first cold Node ownership comparison reported a retained owner for EW01.
Fixed meta-schema arenas are documented instance-lifetime owners. The harness now
warms the complete job and requires exact owner cleanup for every case in a second
complete pass. `cold-owners.mjs` records cold startup separately from repeated
warmed cleanup; these arena counts are not allocation-byte measurements.

## Independent finding and correction

R-EW2 identified an uncharged 33-byte request for the top-level Fails message in
each original pass. Its false/null control requested 361 bytes against 352 charged.
The implementation now charges the shared finite message's exact work and storage
before constructing each F result. Exhaustion before the first completed verdict
returns NoVerdict; exhaustion during diagnostics retains the first F. The original
failing measurement is preserved by the independent implementation review.

The corrected message accounting passes: false/null Outcome requests 361 bytes
against 385 charged; Explain requests 994 against 1,042. Both release all owned
allocations after drop. Native layouts are WitnessSeed 32 / Arc 48 / State 64 /
Frame 248 bytes. Current/MSRV/native small-stack and fresh Node/package/browser
gates pass after this correction.

R-EW3 then found pre-existing underaccounting in the call-local pattern table:
the inline Pattern is 272 bytes, so the first HashMap backing requests 1,168 bytes
against an unexplained 512-byte reserve. The propertyNames/^a$ witness requests
3,105 versus 2,625 charged for Outcome and 6,546 versus 5,586 for Explain.
The correction reuses explicit bounded entry/bucket storage with actual Rust
layouts and metered lookup work. Its private generic name is now Entries; active
stack and prepared-query uses retain their append/pop semantics. No mutable
Pattern borrow spans backing growth. The old constant is removed. All 113 private
library tests pass; the independent property-name and uncertain-guard allocator
reruns now fit the ledger. Actual Wasm layouts are WitnessSeed 16 / Arc 24 /
State 32 / Frame 128 bytes, with zero retained allocation after drop.

R-EW4 remains OPEN at this safety checkpoint: when optional materialization
exhausts its work allowance, constructing and discarding an owned NoVerdict
carrier allocates code/message bytes. The external false-schema control with a
1,000-byte source URI and 1,000 work units preserves Fails but requests 851 bytes
against 818 charged. Larger 10k/100k URI controls show the same deficit, while
funded URI copies fit their reservations. Resolve this internal refusal carrier
before claiming the original Explain resource gate complete. The candidate fix
must avoid speculative owned errors or derive an explicit finite carrier reserve;
an unexplained larger constant is not an acceptable certificate.

Hosted CI for preceding checkpoint 51e1dc7 passed macOS/Linux native, browser,
package, Wasm reproducibility, minimum Rust, Node floors, advisories and performance.
The Windows native job was cancelled after 30 minutes; the required aggregate
check correctly failed. This is not an all-host green claim. Its logs and exact
head SHA are preserved separately.

Independent implementation/resource review, final corrected-source validation and
checkpoint identities are still in progress. Full pattern compiler construction,
remaining physical-stack qualification, outgoing compiler retirement, matched
performance and the final alignment audit remain open.
