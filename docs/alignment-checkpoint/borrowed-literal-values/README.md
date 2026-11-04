# Borrowed exact-value predicates

Const, enum and uniqueness compare borrowed original values. Persistent value
identities remain limited to semantic effect correlation. Arrays retain ancestor
frames; object equality reuses the immutable arena name index and verifies exact
UTF-16 names. Uniqueness verifies all hash candidates exactly. Its private seed
belongs to the immutable prepared Plan so repeated attempts have stable work and
cancellation counts.

The 31-group independent implementation review passes on Rust 1.99 and 1.88 after
closing a real per-call seed reproducibility finding. Strict clippy, exact operand
regressions and the complete 1,546-case evaluator kit pass. The fresh Wasm artifact
passes 188 Node tests, installed-package checks and Chromium/WebKit judgments.
The complete portable verifier and final hosted platform checks are separate gates.

Unchanged allocation requests now measure 488 bytes for the 20,000-number literal,
320,328 bytes for scalar uniqueness and 16,776 bytes for object uniqueness. Full
observations and owner release match. This is allocation evidence, not timing or
G6 acceptance; full paired measurement and explicit residual review remain open.

The no-acquisition witness now expects the current per-value UnsupportedCapability
NoVerdict for an unknown custom profile, instead of the outgoing preparation refusal.
Both HTTP/FIFO positive controls activate; both SDK controls have zero trap hits.
The runner emits failures and CI preserves its report. Independent outcome adjudication
and a separate HTTP/FIFO replay both pass.
