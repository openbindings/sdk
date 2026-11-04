# Owned pattern translation checkpoint

The private HIR factory now owns the complete normalized-form → guarded AST →
checked inventory → logical admission → fresh translation sequence. The parent
validation gates and [independent implementation/resource review](pattern-hir-stage/implementation-review.md) pass. It remains
test-only and does not change the ordinary evaluator or activate a new compiler.
The [design review](pattern-hir-stage/design-review.md) approved this bounded stage.

One exact borrowed form supplies every stage. Prefix/equality limits are checked
before recursive smart construction. Work, allocation and destruction are prepaid;
the translator and AST are destroyed before the completion check. Success retains
only immutable HIR. Cancellation or exhaustion publishes nothing, local refusals
remain distinct from undefined pattern meaning, and future NFA retries borrow the
same HIR rather than translating again.

Reader now enforces the supported authored-group ceiling of 32, which callers may
lower. Normalization can add one atom-local group; its ceiling is separately 33.
An unbounded caller request cannot bypass Reader recursion admission. These limits
are implementation capabilities, not new specification vocabulary. The [construction
policy](pattern-hir-stage/construction-limits.md) replaces the old byte-to-depth
promise with checked logical limits and qualification for recorded builds/artifacts.
No runtime compiler-version allowlist or old-API compatibility requirement is added.

Current and minimum Rust replay exactly the same 11,305 inputs: 11,246 successful
HIRs equal the pinned backend, 51 expected earlier Reader outcomes and eight
expected closed-grammar/profile refusals. All owned bytes are released after drop;
no requested allocation exceeds its prospective reservation. Node, Chromium and
WebKit each pass the actual 128 KiB linked Wasm probe: 11,227 successes, the same
51 earlier outcomes and eight refusals, plus 19 independently predicted checked
32-bit work overflows. Those overflows are refusals, not semantic passes.

Eleven test groups cover the frozen independent boundaries, configuration values,
exact/one-below funding, every observed cancellation checkpoint, nested attempt
poisoning, first-cause preservation, lifetime independence, concurrent owners and
once-only translation. The current library passes 149 tests with two existing
release-only stack tests ignored; minimum Rust passes the 11 focused groups;
strict all-target Clippy passes. [Exact identities and bounded results](pattern-hir-stage/qualification.json)
are retained alongside this checkpoint. Full runnable inputs/probes/outputs remain
in the coordinator's `receipts/2026-11-02-preparation/pattern-hir-stage` directory.

Independent current/minimum replays additionally pass 11,261 corpus/control rows,
3,217 resource/interruption rows and 12 targeted work-precharge rows per version.
Every individual row remains within its reservation and releases tracked ownership.
These counts overlap parent coverage and do not represent one aggregate default job.

The next stage is checked actual-HIR NFA inventory and fully charged fresh retries,
then immutable program sharing and call-local caches. Complete Reader work,
default workloads, supported-build stack evidence, matched performance and final
original-evaluator integration remain open. No merge or release is authorized.
