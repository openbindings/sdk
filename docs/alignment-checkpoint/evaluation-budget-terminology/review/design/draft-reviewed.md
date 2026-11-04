# Evaluation allowance terminology

This greenfield API change follows the independent G5 note in
`../../receipts/2026-11-02-preparation/original-default-performance/root-alias-leaf-memo-review/implementation/limits-terminology-note.md`.
It changes names and descriptions, not defaults, accounting sites, values, outcomes
or evaluation architecture. The current root-memo experiment is checkpointed
separately before this change.

| Current Rust/serde field | Replacement | Type/default |
| --- | --- | --- |
| `evaluation_steps` | `evaluation_sdk_work_units` | `Option<usize>` / `None` |
| `program_bytes` | `evaluation_sdk_reservation_bytes` | `usize` / 48 MiB |

TypeScript exposes `evaluationSdkWorkUnits` and
`evaluationSdkReservationBytes`. The generic camel-case encoder continues to
produce the corresponding snake-case serde fields. Rust serde retains
`deny_unknown_fields`; TypeScript retains its known-key validation. Retired
configuration keys fail rather than becoming aliases. No public compatibility
shim is introduced.

Work units are SDK-defined cooperative debits, not CPU operations, a count of
schema steps or callback frequency. Reservation bytes are cumulative SDK storage
estimates, including proof/memo/scratch/search-cache and diagnostic work; they are
not exact allocator requests, peak live memory, immutable pattern-program size or
a process-memory ceiling. A value call shares its remaining allowance between
verdict and diagnostic passes. Selected-meta evaluation debits both its per-call
evaluation allowance and the enclosing preparation Attempt; exhausting either
can therefore return a preparation Limit. Pattern representation and preparation allowances
remain separately named and owned. Diagnostic codes remain unchanged in this
isolated terminology change.

Update the maintained Rust API, tests, private test fixture carriers, TypeScript
API/tests, generated reference material, current guides, diagnostic probes and
the current alignment-map source bindings together. Preserve pinned inputs,
historical performance tables and committed checkpoint receipts with their
original names and source identities. The historical performance guide already
identifies its API names as historical. Correct the evaluator guide's obsolete
claim that single-build pattern construction is still being implemented.

The private benchmark request protocol stays frozen: its `evaluation_steps` and
`program_bytes` fields retain the original meanings and exact numeric requests.
Only `tools/evaluator-bench/src/sdk.rs` translates these two carrier keys to the
current SDK field names before ordinary deserialization. Reject the replacement
SDK spellings in that frozen protocol even when supplied alone, and reject mixed
old/new keys. Move raw JSON values without numeric conversion, including null
work allowances; ordinary SDK deserialization retains type/range checks. No fixture, expectation,
mode, timed body, quota or result normalization changes. Review the adapter diff
and dependency closure with the existing instrument-difference verifier. A new
final matched campaign binds both instrument identities; an incremental helper
requiring identical instrument digests must continue refusing this pair.

Validation gates:

1. Inspect the complete maintained-source occurrence inventory. Every remaining
   old spelling must be historical evidence, a frozen benchmark carrier, or a
   deliberate unknown-key rejection control.
2. Prove default values and caller overrides at Rust serialization and the
   TypeScript/Wasm boundary. Retired and mixed keys must be rejected. Existing
   work/reservation tests keep their limit values and expected classifications.
3. Run evaluator tests, warnings-denied lint, required evaluator corpus and
   TypeScript type/reference checks. Build the Wasm facade and run Node tests
   against the rebuilt artifact before claiming the boundary is qualified.
4. Audit the private adapter's field-only diff and replay representative default,
   zero, nonzero and nullable work requests and reservation requests through the
   unchanged benchmark carrier. Compare complete observations to its frozen peer.
5. Refresh and verify current source bindings. Final whole-source host,
   performance, package and consumer qualification remains required.
