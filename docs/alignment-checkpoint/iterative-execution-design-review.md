# Independent iterative-execution design review

Decision: **PASS for implementing the complete frame conversion described below; implementation and whole-engine qualification remain open.** This is a read-only design review. No production source was changed. Source identities are in `source-hashes.json`; SDK HEAD at final review is `46186957c2a4494405b92d6e1c5efdd8245347ea`. The existing evaluator used by the independent probes was the copied-source direct-Plan bridge from the final local-syntax review, not the outgoing default compiled path.

The latest concrete continuation proposal is an appropriate minimal architecture. A canonical scope interner is **not required**: content-keyed shared immutable scopes preserve the necessary identities. The design must retain the transition, demand and accounting requirements below. No public API or legacy compiler shape constrains the private organization.

## Authority and observed boundary

Pinned spec `c1419c9` §5.5, lines 453–474, assigns JSON Schema meaning and distinguishes validity, undefined effects and unavailable content; annotations follow each completion. §7.2/7.4, lines 558–595, requires original resource entry, contextual reference resolution and the distinct in-place-cycle result. §7.5, lines 697–699, explains document-resource isolation and outermost dynamic binding. The frame mechanism must change none of these. Existing certified recursive coordinate summaries, exact-value effect identities, four-block dependence test and direct-cycle certificates remain the semantic authority within the supported bounded engine.

`Run::schema` currently combines entry, aliases, Visit classification, recursive application and completion (`semantic/mod.rs:796`). `body`, `object` and `array` contain the recursive applications. `Graph::apply` and quantification remain recursive (`decision.rs:174,231`). Support discovery, direct-cycle recognition, value interning and the actual-meta obligation traversal already use explicit work structures; they are not a reason to retain recursive schema application.

Fresh independent observations are retained without rewriting their expectations:

* `frozen-input.jsonl` / `frozen-observations.jsonl`: the original deep workload refuses evaluation with `semantic-analysis-depth-limit`; the original low-stack control also refuses. Both repeated-reference-text workloads (1,000 and 5,000 sibling references) refuse **preparation** with `preparation-work-units`; reference fan-in satisfies.
* `controls.json` / `controls-observations.jsonl`: 36 additional controls, 32 matching their present acceptance and four depth-32/64 positive/negative controls still requiring restoration. Outcome and Explain agree. These inputs cover applicator folds, exact property names, dynamic scope, aliases, annotations and diamonds.
* Six of the 36 controls characterize the current eager execution at a small program budget. Their schema meanings are separately recorded as S/F; their current Limit results are **not** normative value oracles. A later reviewed demand optimization may improve them. This first mechanism-only conversion should preserve which operations are attempted.

The measurements establish concrete gaps and reproducible controls, not an allocation multiplier, performance baseline or whole-engine PASS.

## Concrete layout and ownership

Use one scheduler with an explicit action/return protocol, illustratively:

```rust
struct Frame<'v> {
    visit: Visit,             // original node, instance occurrence, effective scope
    value: JsonRef<'v>,       // borrowed exact value; never decoded/copied wholesale
    state: State,            // owned validity + demanded annotation coordinates
    phase: Phase<'v>,        // only the current applicator's partial fold/cursors
}
enum Action<'v> {
    Enter(Request<'v>),
    Return(State),
    AbortAttempt,            // provisional-repeat control flow, not NoVerdict
    Failed(NoVerdict),       // terminal cancellation/limit/other failure
}
```

These are private illustrative types, not a required API. The important representation is one reusable vector of suspended frames and one owned child-result slot. Do not box each continuation, retain a vector of every child, or clone a State merely to cross a return boundary. Move the completed child into its parent fold. A current frame may be moved temporarily out of the vector while dispatching helpers, provided its Visit remains in the ordered active path/index until actual completion or abort.

Read the original schema using the Plan node index rather than embedding a second schema tree in the frame. Preserve original input occurrence for cycle detection; exact-value interning remains a separate equality mechanism for effects. A repeated equal value at a different occurrence must not become an active recursive Visit. The prepared Plan retains original owners; call-local frames and scope state cannot retain preparation controls or escape into a prepared cache.

A certified leaf runs entry/tick and the existing predicate body directly. It retains no Frame, Visit or complete memo entry for each scalar item. Preserve the zero-producer leaf certificate used by refinement; absence of a map by itself never proves absence of annotations. Non-leaf diamonds still reuse a completed State for the exact Visit.

`backend::member_at` already supplies the needed O(1), allocation-free object cursor (`internal-json/src/backend.rs:48`). It indexes the arena's member-name vector and returns the original name token/value. Use a scalar index; do not add a public iterator abstraction or implement repeated `nth` scans. Existing duplicate-name/non-scalar-name tests remain necessary. Array and schema-branch elements already have O(1) indexed access.

## Continuation coverage and meaning

Every route that currently calls `schema` must become a child request, including the less obvious whole-instance and name-token calls:

| Phase family | Retained state and required return rule |
|---|---|
| References | Reference keyword/next keyword; consume the prepared query and per-call dynamic selection. Same-instance target annotations propagate. A draft-07 reference object completes with the reference result, ignoring sibling evaluation. |
| allOf/anyOf/oneOf | Branch index and combined State; retain the current exactly-one zero/one formulas. Evaluate every branch initially. Merge annotations under the applicator's existing guards, then combine validity. |
| not | One child; negate validity, discard all child annotations. |
| if/then/else | Condition plus arm results/guards. Both present arms are currently evaluated; unchosen-arm limits must not silently disappear in this conversion. Preserve condition annotations and the existing arm gating. |
| dependentSchemas/legacy schema dependencies | Dependency cursor; invoke child on the entire enclosing object only when the exact trigger member exists. Array-valued dependencies remain native primitive work. |
| properties/patternProperties/additionalProperties | One member cursor, optional demanded exact UTF-16 name, selected formula and pattern cursor. Child instance is the member value; nested child coordinates do not become parent coordinates. Current pattern children and additionalProperties children are attempted even when their match/selection makes validity irrelevant. |
| propertyNames | Child instance is the original member **name** JSON token, not its value. Only child validity contributes. |
| unevaluatedProperties | A second member cursor reads the enclosing accumulated annotation formula and guards the child assertion. Preserve current evaluation even for a definitely evaluated name. |
| prefixItems/items/legacy tuple/additionalItems | One item cursor and active keyword/profile rule; selected child result affects validity and that parent's item annotation. No tuple collection allocation. |
| contains | Item cursor plus existing concrete-match offset and uncertain distribution. Preserve correlations and 2020 annotation formulas; 2019 contains annotation behavior differs. |
| unevaluatedItems | Item cursor and accumulated item annotation; preserve guard and present eager child execution. |
| Complete | Gate every coordinate by final validity, test cut independence, stage coordinate facts, and only then classify complete/provisional. |

Retain the current reference/logical/object/array sequence and all typed refusal distinctions initially. Invalid patterns/references remain coherent semantic effects where appropriate; an actual resource limit or cancellation is never converted into an effect or ignored because another branch is decisive. The first rewrite may continue using current helpers for exact predicates, counts, pattern matching and formula operations.

## Scope and membership

`Arc<Vec<(String, Option<usize>)>>` is sound and simpler than introducing mandatory ScopeIds. The vector must remain sorted and immutable; Eq and Hash inspect its **contents**, not Arc address. `None` is an established ambiguous dynamic binding, not the absence of a name. Outermost binding wins. Two paths producing equal effective maps must compare equal; equal original nodes with different effective maps must remain different Visits.

`enter` can share the incoming Arc after inspecting the actual original resource and determining that no new visible binding is introduced. Include 2019 `$recursiveAnchor` activation at the current node in that decision. A cache keyed only by resource would be wrong if it ignores that node-sensitive activation. Changing the map must reserve the new vector, copied retained names and other owned contents before allocation. Sharing an unchanged Arc does not allocate a new vector. Charge text/hash/equality scans as work even where allocation disappears. No linked parent-chain representation is needed; such chains would introduce lookup and recursive-drop concerns.

Keep an ordered active path and `Visit -> active position` index. An active repeat needs the exact suffix, not just a membership Boolean. Keep alias path order and a `(original node, effective context) -> first position` index too. Do not scan the whole previous path on every edge. Alias projection must still retain the dynamic names actually queried in a pure cycle; preserve malformed-identifier, unsupported-inventory and profile guards. Reference spelling or normalized URI is never sufficient identity.

Index growth and key Arc clones require explicit accounting. Content-keyed hashes on long scopes are not O(1); charge them or later introduce a separately reviewed canonical interner. Standard library map layout is not proven by a similarly versioned registry `hashbrown` crate. Any production allocation schedule must be justified against the supported standard library/layout or use an independently bounded storage mechanism; do not assume the old per-Visit allowance automatically covers a second table.

## Completion, abort and retry invariant

1. Entry checks the call control/work and original interpretation. Complete memo precedes opaque handling as today. Active repeats obtain recursive cuts from previous-round facts/support. Provisional repeats request `AbortAttempt`; they never return the prior partial State.
2. A normal completion gates **every** property/item coordinate with final validity. A validity-TRUE State with a cut-dependent annotation is not complete. Only cut-independent complete States enter the complete memo. Coordinate facts are staged only after a frame finishes.
3. A child error/abort abandons suspended parent partial folds. It must not run their completion postlude, publish their annotations, or certify a formula they never completed. Earlier fully completed frames may have already produced valid complete memo entries or staged coordinates.
4. `AbortAttempt` clears the return slot, frame vector and active path/index, then invokes the existing progress test. It is not cancellation or a resource failure. Terminal errors clear call-local execution state and propagate without retry. All paths must remove active membership consistently.
5. Between attempts retain exactly the existing cumulative Graph/atoms/effects/values/cuts, completed memo, facts/opaque states and call-local prepared search scratch. New pending facts become visible only at the commit boundary; no frame reads its current-round pending facts. Support bounds are invalidated on complete memo growth. No-progress remains honest inability. No budget reset, partial successful cache publication or conversion of opaque inability to Undefined.

Shared scopes referenced by retained keys naturally live across retries. If an optional future scope interner is introduced, its IDs cannot be recycled while any such key exists.

## Allocation and machine stack

Reserve checked vector capacity **before** growth. With a cumulative allocation ledger, charge the full newly requested backing allocation, not merely `new_capacity - old_capacity`: the old reservation has already been spent and the allocator may have both buffers live during relocation. Reuse existing capacity without a new backing-allocation charge. Separately account State maps/names, phase-specific count storage, scope ownership, active/alias indexes and key copies. Size each frame from the actual enum/layout; do not store every phase's partial State simultaneously. Retain a logical work charge for dispatch/lookup even when empty maps/scopes allocate nothing.

Remove schema nesting as a machine-recursion depth limit once **all** schema applications use frames. Heap frame growth remains bounded by program memory. Do not relabel the old depth heuristic as a new frame count or spend `stack_bytes` as heap allowance. The fixed scheduler/predicate stack floor and remaining symbolic recursion are separate obligations.

`Graph::apply` and quantification still recurse in variable order. Quantification can call apply while its own recursive frames remain live, so their combined live depth must be covered. Existing Graph variable limits may remain conservative during the rewrite, but they are not proof of the fixed scheduler's total machine stack. Preserve low-stack refusal controls; audit native and Wasm layouts/emitted stack, test small native thread stacks, and retain a justified floor before claiming stack qualification. A finite passing allocation/stack probe cannot establish an arbitrary coefficient. No new floor/variable-count number is approved by this design report.

## Smallest complete implementation and gates

1. Implement shared immutable contexts, bounded active/alias membership and frame-capacity helpers with identity/overflow/cancellation controls. The indexed object accessor is already present.
2. Replace schema application by one complete scheduler covering every phase above. Retain existing primitive/BDD/support/cycle helpers under their own bounds. There must be no recursive `schema` fallback for custom, dynamic, conditional, object, array or recursive cases.
3. Independently replay all 1,546 required values through direct Plan, strict 505 core and 2,348 adjudicated auxiliary obligations, all accepted recursive/coordinate/private-external/foreign/custom/meta/exact-value controls, plus these 36 controls. Add instrumented non-leaf diamond completion count, deep real reference chains, alias chains and the same target under distinct/equivalent scopes. Instrumentation must not redefine the outcome oracle.
4. Enumerate cancellation at every observed entry/return/commit/abort boundary, test one-below/exact growth and arithmetic overflow, then clean retry on the same prepared owner, concurrent reuse and no retained instance/attempt allocations. Mutants to reject include early active removal, memoizing validity only, merging not/propertyNames coordinates, using value identity for occurrence recursion, scope address identity, skipping 2019 entry and treating AbortAttempt as a semantic result.
5. Restore original 20,000-valid/20,001-late-invalid/50,000-object jobs, deep valid/invalid workloads and small-stack refusal at unchanged defaults. Measure allocations separately from matched timings. Qualify current/minimum Rust, actual Wasm/Node/browsers and installed/package consumers before a later default-engine switch.

OE-04's deep application gap is directly targeted. **OE-05 remains a whole-engine gate:** the frozen 1,000/5,000 reference-text fixtures are repeated sibling spellings, not deep chains. Current `check_from` still charges reference bytes per referring position and local fixed syntax still reads those reference strings. The 5,000-case reference-byte charge alone exceeds 20M work. Frames cannot fix preparation refusal. Contextual static-query sharing and source-tied fixed-syntax fact/read sharing need their own identity/accounting designs; no expectation or default-cap change is justified here.

OE-06 full pattern VM-once, cross-phase syntax-fact sharing, OE-08 diagnostic witnesses and the outgoing ordinary compiler allocation/removal gate stay open. This report approves an executable bounded frame design, not final evaluator alignment or performance completion.

## Follow-up: explicit LIFO membership storage

The proposed private `Path<K>` with two vectors is an acceptable alternative to a standard HashMap: ordered entries hold the semantic key, stored hash and prior bucket index; bucket heads use an empty sentinel. These active/alias paths only append and remove in LIFO order, so they need no tombstones or general mutable-map semantics. Rehash must rebuild every prior link and head in retained entry order; pop must remove the current head for its bucket, and clear must reset every head before reuse. Keep stable integer indices across vector relocation, not borrowed pointers. Full key equality must distinguish ordinary hash collisions, and each collision-chain traversal/content comparison plus rehash work must be charged. Adversarial collisions can cause an honest work refusal, not unchecked work or false Visit equality. Reserve checked full new capacities for both concrete layouts before allocation. This is a necessary private bounded-storage mechanism, not a new semantic identity authority. Its actual implementation still needs collision, grow/pop/clear/reuse, exact-cap and cancellation tests.
