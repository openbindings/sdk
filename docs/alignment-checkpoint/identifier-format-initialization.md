# Regex-format initialization correction

The original evaluator's regex-format selector retained a `OnceLock<()>` and a
2 MiB / 10,000-work warm-up reservation after identifier membership moved to
generated immutable ranges. Those membership calls now perform scalar checks and
binary searches only. The flag, synthetic reservation and warm-up calls are
removed. This is an obsolete implementation assumption, not an interface to retain.

The selector still charges its input-sized work and observes cancellation. Actual
regex syntax checking keeps its per-input policy. The other fixed format validators
retain their separate construction reservations and `COMPILED` publication state.

The focused regression failed against the prior implementation, then passed after
the correction. Ordinary and escaped format names both prepare twice with zero
allocation allowance and exactly the required work. One fewer work unit refuses;
an already-cancelled caller refuses. This establishes reservation/source behavior,
not a separate global allocator measurement.

Current Rust 1.99 and minimum Rust 1.88 each pass 115 library tests and 16 custom
profile tests. The two release-only frame tests are intentionally excluded from
those debug runs; their separate qualification remains in the portable verifier.
Strict evaluator lint passes. A fresh Wasm build passes all 187 Node tests. The
first restricted Node run could not open its local HTTP test servers; the rerun
with local server access passes. No failed assertion was removed.

The independent reviewer inspected the exact production diff and reran the
focused regression successfully, approving this bounded correction. Reviewed
`semantic/formats.rs` SHA-256:
`e340bab36fe1897977a931cb9f8d5c8a3ec2d56fc80b2150427d507fab1c7c6d`.
Full logs, the prior failing control, build identity and the independent receipt
are in the coordinator's
`receipts/2026-11-02-preparation/original-stack-qualification/` directory.

This does not close the full pattern compiler, physical-stack or final SDK gates.
