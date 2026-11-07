# Guarded normalized pattern inventory

A private, test-only scanner now checks the Reader's normalized token subset and
collects checked structural counts without allocation, recursion or backend
parsing. It admits a guarded superset of emitted token sequences; it does not
re-adjudicate ECMA validity. Its fixed ASCII-boundary wrapper counts toward actual
normalized depth. Classes must be positive, flat, ordered, nonoverlapping and free
of surrogate interiors. Unsupported syntax cannot reach later backend exclusions.

The scanner precharges a checked linear work allowance, checks cancellation every
256 consumed bytes and at completion, and preserves prior sticky failures before
arithmetic refusal. Success and refusal retain only borrowed input/fixed data.
The compiler module is enabled only for unit tests: ordinary evaluation, public
APIs and package behavior are unchanged. This is not full prepared-VM activation.

Validation of this slice:

- Current Rust: all 121 evaluator unit tests pass; two separate release-stack
  witnesses remain intentionally ignored in this debug target. Strict all-target
  evaluator Clippy passes.
- Minimum Rust 1.88: all six new groups pass, including 1,142 unchanged Linear
  forms, 60 independent direct controls, exact/one-below work, group-depth,
  cancellation in long loops, and sticky-overflow precedence.
- Independent implementation/resource review passes: 40 allocator lanes on each
  Rust version request zero bytes, including 65,536-byte inputs and failures;
  271,465 loop/read controls fit the bounded scan-work term.

The independent report and exact source hashes are retained in the coordinator's
`receipts/2026-11-02-preparation/prepared-pattern-program-review/normalized-implementation/`.
Input provenance is beside the checked-in tests. The active compiler design is
`design/2026-11-02/prepared-pattern-program-schedule.md` in the coordinator.
Remaining gates include AST/HIR/NFA work, iterative checked metadata, physical
compiler stack and combined default workloads before any runtime activation.
