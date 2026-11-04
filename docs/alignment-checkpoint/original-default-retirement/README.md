# Original evaluator default: recovery checkpoint

The SDK now prepares and evaluates through one original-schema plan. The outgoing
projection compiler, bounds copies, stand-in compiler, bundle specialization,
stack-depth conversion and unevaluated-walk transform are deleted. Pattern
programs are prepared once and shared immutably; search caches belong to a call.
Public `compile_json_depth`/`compileJsonDepth` and `stack_bytes`/`stackBytes`
options, and the conversion-check feature, are removed. No old API is preserved.

This is an unfinished feature-branch checkpoint, not completed qualification.
It targets specification `c1419c9fc3ae5a00c2b83da710c11bf67dab7934` and interfaces
`5815fb106bddd2c85ff4e93e0077570b3f6b9e74`.

## Behavior and evidence

The ordinary default passes all 1,546 required evaluator cases. Focused library,
contracts, diagnostics, resources and exact-number checks pass at the recorded
runs. The [independent integration review](independent-review.md) covers 71 checks
on both supported Rust versions, including adjudicated changes in witnesses,
reference identity, cancellation and unsupported capabilities. Historical inputs
and dispositions are retained; the required suite is unchanged.

Two capacity corrections preserve existing positive workloads. Inline signed
integer predicates avoid heap scratch allocation, and reference-free plans avoid
unnecessary completed-application memo entries. The unchanged 700,000-item array
and 262,143-item multi-assertion workloads pass their recorded controls. The
standalone integer probe measures zero allocations across 60,000 decimal reads
and 300,000 bound checks. These observations are not a whole-SDK performance claim.

The unchanged 380-pattern workload (10,002-byte anchored literals) now prepares
under default budgets. A narrow Reader path produces the same interpretation;
the construction index hashes forms once and checks exact text for collisions.
Work and reservation charges, collision rollback and cancellation remain bounded.
The general Reader and the 6,000-shorthand early-refusal controls remain in place.
[Independent literal/index review](literal-review.md) passes on both Rust
versions, including 1,339 full Reader comparisons, forced hash collisions,
53 cancellation checkpoints and the actual default large workload. Measured peak
is 7,757,588 bytes, with live allocations returning to baseline after owner drop.

Symbolic reasoning has a private maximum of 64 effects, shared by ordinary,
meta-schema and Explain passes. Tests cover 63/64/65 effects, coherent reuse,
earlier failure/cancellation precedence and strict rejection of removed options.
[Independent effect-policy review](effect-review.md) passes seven public groups
on each Rust version. Concrete nesting is separate from this logical bound. The private sampled stack
guard does not promise portable stack capacity.

The subsequent [capacity and host qualification](capacity-qualification.md) resolves
the large-payload findings and the remaining preparation policy assertions, with
fresh Node and browser results. The list below records the earlier recovery point.

## Gates recorded at the earlier recovery point

- Finish preparation-scaling test adjudication. Preserve required large-payload
  positives and investigate actual capacity losses, rather than accepting a new
  refusal because the candidate happens to return one.
- Finish broad current/minimum Rust, release stack, actual Wasm/Node/browser,
  package/reference/consumer and hosted CI checks for this default build.
- Complete matched performance against baseline `18a66957d038431a5caf0db340d96f60bc1decdf`
  using the same toolchain and protocol. Classify changed policy outcomes explicitly.
- Complete the independent default-switch review and final spec-to-code audit.

[Receipt index](receipts.json) identifies the bounded logs saved here. Broader
initial failures and work-in-progress experiments remain in the migration
coordinator's `receipts/2026-11-02-preparation` directory. No merge, release or
publication is authorized by this checkpoint.
