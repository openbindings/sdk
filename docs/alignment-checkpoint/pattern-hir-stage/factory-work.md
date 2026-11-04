# Fixed owned-HIR factory work

The fixed schedule is 128 scalar operations plus four logical target-word transfers
for each of Parsed, Inventoried, TranslatorBuilder, Translator, backend
Result<Hir,hir::Error> and local Result<Hir,Error>. Native measured widths are
112/368/8/40/80/112 bytes, so the fixed charge is 488 units. Use actual `size_of`
with checked sums and multiplication, not a pointer-width scaling of native bytes.
This bounds source-level operations/movement, not CPU instructions or stack bytes.

The six-entry fixed layout fold accounts its own ceiling divisions, multiply,
addition, iteration and failure branches. The remaining scalar envelope covers
fixed depth derivation, admission comparisons, checked work/allocation additions,
control-call dispatch, fresh translator configuration, error classification and
completion branching. Each owner gets up to four local/argument/result/drop-slot
transfers; both returned-error and successful-HIR slots are covered. The existing
AST and inventory schedules already cover their tree/frame traversal and cleanup;
the HIR inventory covers translator traversal, heap ownership and HIR destruction.
No unbounded input traversal is hidden in the fixed batch. Every control call also
applies its own normal check/ledger cost; these are not subtracted from the batch.

The one-form-byte work/allocation addend reserves the translator's defensive error
source copy, although the locked closed-grammar source proof excludes every such
error constructor. The unexpected error is dropped without formatting. Its owned
slot movement is already in the fixed schedule. Ordinary success retains no form
copy or dependency error. Whole-job totals must include the defensive reserve.

Parent requested-allocation probes include full AST/inventory/translator cleanup
and final HIR drop. Comparing HIR and running independent configuration values
occurs with measurement disabled and does not add probe work to the factory.
Current/minimum measurements are identical. Wasm expectations preserve19 independently
predicted work-sum overflows; none is counted as successful HIR translation.

Initial harness failures are preserved: the unit adapter attempted a nonexistent
PikeVM build_from_hir convenience method; the corrected adapter explicitly builds
NFA then PikeVM. An expectation-string test misclassified independent Reader Depth
controls; it now matches exact typed outcomes. The first standalone Wasm link
omitted the host dependency lookup directory needed by its procedural dependencies;
that build produced no passing artifact and its log is retained separately.
