# Original Explain implementation sequence

This implements the approved original-explain-witnesses design and its independent
42-control inventory. The direct exact-pointer construction prerequisite is now
qualified in a6b3bea. It does not require the still-open full pattern compiler
certificate. Existing private signatures may change; no compatibility adapter is
needed for the outgoing compiler's diagnostic implementation.

1. Add private remaining-work/remaining-requested-byte observation to Graph. An
   execution completion carries its outcome and actual remainder, never fresh
   configured maxima. Move the existing root execution/retry loop behind a private
   completion boundary so every exit can account for the Graph it used. Preserve
   stack observation and unavailable-reference recording for custom-meta admission.
   The ordinary pass runs without witness creation. Only its established Fails
   under requested Explain can start the second pass; the first run's owners drop
   without refunding cumulative charges. All passes borrow the same external
   control and enclosing preparation Attempt.
2. Add Option<Arc<WitnessSeed>> to State. A seed holds original schema ID, optional
   static keyword and actual instance arena node ID. The call owns the input arena;
   Plan owns the schema environment. Seeds do not own controls or enter Plan.
   Charge actual target Arc storage before creation; measure State/Frame layouts.
   State copies only clone the Arc. Every completed State with a seed is concretely
   FALSE. Partial refinement facts, opaque summaries and cut identities carry none.
3. Change primitive aggregation to retain the first failed static keyword while
   still executing every existing check in the same order. Integrate Boolean
   false, primitive, format and pattern seeds into execution::begin. Later resource
   or cancellation failures must still abort the diagnostic pass, preserving the
   already-established first verdict rather than publishing partial false evidence.
4. Propagate seeds through unconditional conjunction/reference/item/property and
   dependent-schema continuations. allOf may forward a false child's seed; anyOf
   and oneOf name their own applicator only after final false aggregation. not
   names itself when its child is TRUE. A concrete if selects only its active
   branch's false seed. Keep correlated contradictions without a local witness
   unexplained. Do not attach the failed condition as if it were an assertion.
5. Guard object/array propagation with actual TRUE applicability. The interpreter
   may visit guarded children eagerly, but those visits do not prove application.
   propertyNames records the containing object and its own keyword. contains
   records its cardinality applicator, not an individually nonmatching item.
   Complete independent false memo states can retain their own exact-occurrence
   seed; partial recursive support/refinement cannot create one.
6. Materialize only a selected root seed after the second pass itself concludes
   Fails. Validate same-arena ancestry against the actual supplied root before
   calling the ancestor walker. Charge traversal, exact instance pointer, original
   schema/source pointer and URI, keyword suffix, fixed message/code, final problem
   vector and any copies from the same remaining Graph. The now-direct pointer
   writer does not allocate intermediate ancestry; callers still need size/work
   measurement and cancellation. Failure at any point leaves the first Fails with
   no speculative detail. Keep problems_complete=false and use finite SDK messages.
7. Replay the independent controls with exact schema/instance provenance, all
   pre/post-verdict cancellation points, cumulative exact/one-below work/bytes,
   source-size/ancestry rejection, actual allocation/cleanup, concurrent owners,
   and Outcome's zero witness allocation. A healthy second-pass S/U divergence
   from first-pass F is a test failure even though the public fallback stays F.
   Then run all required original cases, unchanged frozen Explain/performance
   workloads and actual Wasm hosts before asking for implementation qualification.

This first-witness scope is intentionally bounded. Reconcile public diagnostic
detail promises and documentation during default-engine replacement; historical
error wording, library-shaped fields and private entrypoints are not requirements.
Do not describe this increment as the default switch or whole-SDK completion.

The independent fixture itself contains exact lone-unit expected pointers
(EW39), so the Rust harness must read controls with JsonValue, not serde_json's
scalar-only Value. Use the supplied-source path for each original schema to keep
its source coordinates and permit exact external schema names; force Plan directly
inside private tests without rewriting schemas into custom profiles. Preserve
schema_text/value_text overrides and the actual evaluate_subtree root. Compare
pointer UTF-16 units and original source URIs, then resolve the pointer against
that exact instance/source. Never normalize an oracle's lone unit or prepend a
synthetic source prefix merely to make a location comparison pass.
