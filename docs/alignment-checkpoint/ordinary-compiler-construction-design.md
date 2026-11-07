# Ordinary compiler construction: executable reservation design

**Design recommendation; ordinary compiled allocation qualification remains OPEN.** This refines the previous failing allocator report into separable implementation gates. It does not assert an unaudited backend byte multiplier. No production edits or new allocator experiments were made for this addendum. Current source and locked backend identities are in `source-hashes.json`.

## 1. Make path selection precede optional compiler work

Add a private, bounded `CompilationRoute` calculation from the original environment, before `SchemaSpace::program_definitions`. It returns either `Static(CertificateInputs)` or `Semantic(reason)`; choosing Semantic is not a preparation failure or an Undefined verdict. Reuse original inventory/profile/reference semantics and the already bounded semantic preparation walk. The selector's own queue/visited storage and queries use the outer Attempt. Do not use a vendor registry to make this decision.

For the first allocation-qualified slice, route these reached shapes directly to the existing semantic Plan:

- Any unevaluatedProperties/unevaluatedItems holder, including Boolean schemas and a single holder above an otherwise acyclic diamond.
- Active dynamic/recursive reference dispatch, a reached in-place cycle, or dynamic specialization.
- Missing/undefined reference effects, invalid/unsupported patterns, or any other stand-in requiring old lower/upper program construction.
- patternProperties and pattern forms needing compiler sharing/retries until their separate reservation schedule is implemented.
- Foreign/custom interpretation, stopped inventory, and exact non-scalar forms already handled by the semantic route.
- Any backend keyword combination not on the audited static whitelist.

This is a conservative implementation route, not a new semantic capability refusal. A funded semantic preparation/evaluation must still meet the original S/F/U oracle. All performance gates remain unchanged: if a routed workload regresses beyond a required gate, add and qualify the appropriate static lane before closure; do not waive its outcome or timing requirement.

An initial static whitelist can cover Boolean schemas, type, exact numeric/const/enum/count predicates, required, properties, allOf, not, and resolved acyclic static references. Exclude additionalProperties fusion, array-shape fusion, anyOf/oneOf discriminator construction, dependent schemas and regex engines until their extra compiler allocation sites have schedules. These are not intrinsically unbounded; they are separate audit increments. This whitelist alone is **not** permission to enter the backend: it also needs the certificate in section 5.

Keep plan selection private in `prepare_attempt`; do not add a public evaluator abstraction. `Plan::new` must still run its actual-meta/profile/admission checks. A forecast that is unsupported can choose Semantic before allocation. A failed `Attempt::charge_allocation`, arithmetic overflow or cancellation is sticky and cannot be converted to a clean fallback or a fresh budget.

## 2. One control through the owning boundaries

Pass the same borrowed internal Control through these functions, preserving structured failure mapping:

| Owning stage | Functions to change | Allocation authority needed |
| --- | --- | --- |
| Reach and projection | `SchemaSpace::{reach_unchecked,check_cycles,fields,program_definitions}` and the text fallback in projection | Queue/set/edge/target storage, generated address strings, field/definition vectors, copied literal arena; charge before allocation |
| Conversion | `DefaultEvaluator::{prepared,defined}`; `Operands::{definitions,field,number}`; `read`, `direct`, `pattern_source` | Every serde map/array/string, number parsing and exact operand table, enum handle array, sources/numbers maps |
| Bundling | `bundle::{bundle,Bundler::merge,document,document_with_bounds}`, specialization and cycle transforms | New indexes/edges/names; copies and reconstructed containers, not just byte growth |
| Patterns | `patterns`, `Kept`, `Operands::pattern`, ordinary matcher build and `share_names` | Existing qualified reader preflight, every compile attempt's scratch, form ownership, pattern caches |
| Backend entry | `DefaultEvaluator::compile`, `bounds` and `share_names` before **each** `options.build` | One complete version-locked build certificate per attempted build |
| Diagnostics | `Locations::new`, `keyword_location`, cycle-origin collection, `preparation_error` | Measured original pointers/URIs before materialization; bounded failure origins |

`Compiled`, `Patterns` and a returned validator never retain the Attempt/control. Keep typed Limit/Cancelled/Resolution causes. Outer final checks remain necessary, including when an optional bounds build says unavailable. The source presently loses this authority at `prepared(..., WorkControl)`; fixing only `options.build` leaves the already-reproduced zero-charge projection hole.

## 3. Concrete SDK reservation recipes

The following are construction recipes for the supported 32/64-bit targets, not a global multiplier for arbitrary backend code. All arithmetic is checked. A failed checked product sets the outer allocation refusal. Count a new allocation at its full requested size, not the increase over a previous buffer, because this is a cumulative allowance. No refund follows drop/shrink.

Prefer explicitly sized construction after a bounded counting pass:

- `Vec<T>::with_capacity(n)`: reserve `n * size_of::<T>()` before creation. If converting to Arc/Box can reallocate, reserve the destination independently. For a dynamic SDK buffer, explicitly choose the next capacity and reserve that **whole** new buffer before reserve_exact; no implicit unchecked growth remains.
- `String`/UTF-8 buffer of maximum b bytes: reserve b. For decoded UTF-16, use an exact-capacity Vec with `2 * unit_bound`; do not collect through an unknown-size iterator.
- `Arc<T>`: reserve the aligned allocation containing two atomic counters and T. A conservative supported-target expression is `align_up(2*size_of::<usize>(), align_of::<T>()) + size_of::<T>()`, rounded to the allocation alignment. Strings/slices substitute their full payload size.
- Insertion-only BTreeMap: for n inserts reserve at most `2*n*align_up(128 + 11*(size_of::<K>()+size_of::<V>()), max_alignment)`, plus the dynamic key/value payloads. The 11 entries and 12 child edges are the Rust B-tree node layout; 128 covers parent/length/alignment and twelve 64-bit edges on supported alignments <=16. This is a cumulative prefix bound for insertion/splitting, not a deletion/reinsertion bound. Terminal filtering/draining performs no new insertion. The same assumption already underlies the separately reviewed comparison maps. Add layout guards and retain the exact toolchain coverage.
- Hash tables: do not silently apply that B-tree formula. For the pinned Swiss-table family a conservative insertion-prefix recipe is `8*(n+1)*(aligned_entry_size+32)`, with key/value payloads separate. Its derivation is final buckets < roughly 16n/7, total geometric bucket allocations < twice that, plus control bytes, alignment and group overhead. It is valid only after verifying the actual std/ahash/hashbrown table implementations used by the qualified target/toolchain. For SDK tables where that audit is undesirable, use explicit-capacity retained storage with a separately audited index. Hash collisions may add work and collision-list storage; they never justify omitting those lists from the count.

Do not hardcode the final sizeof values in general-purpose code when the SDK type is available. Use `size_of`, guard supported alignments, and reserve dynamic payloads separately. These recipes cover allocation requests, not allocator physical/RSS overhead.

### Projection counts before construction

Let N be reached schema nodes, R reached resources, E emitted child/reference occurrences, F retained field slots, L separated literal values, and B_l the complete `[literal,...]` text size. Generated numeric names/addresses use explicit digit bounds from the actual node/resource numbers; do not use original reference length as a bound on an expanded address. Enumerate emitted occurrences, not unique target identities.

`fields` reserves its field capacity and each map/list capacity before traversing it. Each generated address is charged once per occurrence, including temporary resource URI formatting. `program_definitions` separately charges sorted node IDs, resource grouping, names, final resource/definition vectors and the entry address. `DefinedResource.bytes/nodes` currently arrive **after** field/address allocation and cannot retrospectively pay for it. The fallback that writes projection text must reserve that buffer before entering `projection`, even if used only to confirm an admission limit.

For `standalone_list`, use a dedicated counted parse reservation before creating the concatenated text. A deliberately conservative candidate for the current raw parser is:

`A_literals = 4096 + 6*B_l + 1024*N_l + 64*L`,

where N_l includes the wrapping array, every copied value and object-name node. Charge zero for the empty list. Derivation: two complete source copies; geometrically grown decoded bytes bounded by source size; Node records <=32 B on supported targets; items/pending/decoded-end/duplicate/unpaired vectors; all object-member hash buckets and worst-case collision lists; member-index records; the parser's 64-item initial pending allocation; final JsonValue-handle collection and fixed arena/header storage. The 1024/node envelope covers these **enumerated parser structures**, not arbitrary compiled validators. Verify raw parser layout/table guards and the original exact/duplicate/collision controls before using this candidate as a qualified certificate. A changed raw parser invalidates this recipe's source hash.

### serde conversion and exact operands

Define `J(v)` structurally from the actual emitted Value tree: each string/key's decoded byte length, each array's reserved Value capacity, and each object's B-tree insertion bound. Scalar Value storage belongs to its parent container and is not counted again as a separate heap object. Use full occurrence counts for clones.

The current `read` first attempts `direct` and, on a late number/non-scalar string, abandons that partial tree and starts a text deserializer. Either make the certified lane's conversion infallible by classifying first, or reserve both constructions and parser scratch: at least `2*J(v)` plus a separately bounded source-sized decode buffer. Do not assume only the final successful tree was allocated. Prefer exact typed conversion for this lane so no second recursive parse is necessary.

`Field::Map` emits one object and, per member, a key plus a `{$ref:address}` object; `Field::List` emits one array and that reference object per element. `Single` emits one reference object. `Reference` emits its owned string. `Const` retains a literal handle; `Enum` allocates its complete choice-handle Vec and the Arc slice conversion separately. Numeric operands use the existing exact Decimal preflight derived from token length (not expanded exponent value), including operand-map growth. A one-million-digit exponent must be refused or paid for before parsing.

### Bundles and diagnostics

For the static route, `Bundler::merge` can move definitions rather than clone their contents. Reserve its indices/names/out/definition vectors and every newly generated reference/edge. `Bundle::document` currently consumes the bundle: charge the new root and `$defs` map nodes/keys, not a fictitious whole-tree clone. All transform and `bounds` routes remain semantic until their actual clone recurrences are charged.

For `Locations::new`, first measure original pointer length P_i and segment count S_i for every distinct original definition; separately count distinct source-URI bytes U. Allocate the joined text and tables exactly once. Its reservation is the concrete sum of text `sum(P_i)`, URI copies U, entry/vector/map storage, bounded pointer-writing scratch, and exact Pointer segment storage where needed. A deep source can have `sum(P_i)` quadratic in input depth; source bytes or unique node count alone is insufficient. Repeated generated copies map to one original definition and must not duplicate its stored pointer.

On failure, omit origin for Limit/Cancelled. For other failures measure a <=64 KiB original location before construction and reserve the measured pointer/segments/URI before copying. Never call `space.location` first merely to discover it is too large. This closes the retained 100k-name refusal witness without changing its failure kind.

## 4. Explicit expansion recurrences

For an acyclic projected graph, count duplicate edges in `W(v)=1+sum(W(child))`. This is an allocation overapproximation when a compiler cache shares completed nodes; do not deduplicate the arithmetic merely because the target is shared. Saturate at the remaining certificate bound, returning route-not-certified rather than constructing the expansion. A chain can also create deferred alias-state copies: `Context::defer` clones `alias_placeholders` into each DeferredTarget (compiler.rs around 991–1007). Charge `sum(active_aliases_at_defer)`; a safe coarse graph bound is references times maximum reference depth, not just references. The static lane must pay for or avoid that term even without unevaluated keywords.

For any future unevaluated backend lane use a **separate** walk recurrence per holder and properties/items mode. Each follows the locked validator's actual annotation-producer traversal, preserving duplicate edges and conditional/dependent paths. Total walk cost is the sum over holders and builds, not the maximum holder. For a two-edge diamond, `W_d=1+2*W_(d-1)=2^(d+1)-1`; the earlier depths 4/6/8/10 measurements are an independent lower-bound witness. The library's pending annotation cache is removed after completion, so ordinary completed-node sharing does not reduce this term. Routing all such holders to Plan is the simpler sound first implementation.

## 5. Backend certificate: what remains to prove, and how to make it executable

Even the static whitelist cannot use a naked “C*nodes” reservation. Build a private version-locked allocation-site schedule for the **actual emitted** one-resource bundle. Reject any emitted shape not recognized by that schedule before `options.build`. Required schedule components are:

1. **Generated-schema meta validation.** `build_validator` calls `validate_schema` by default; `without_schema_validation` is crate-private in 0.58.6. Charge the existing qualified fixed-2020 cold-cache allowance (16 MiB when initialization is needed), and a per-input validation allowance before this hidden pass. The original conformant proof does not disable it. Warm state only skips allocation that is actually already initialized; use a synchronized initialization gate.
2. **Registry construction.** The bundle must have its fixed root id/dialect, no nested resource IDs/anchors/custom vocabularies, and only known local `$defs` references. Schedule borrowed document records, traversal/visited/deferred-reference tables, generated URI parsing/normalization and the fixed packaged-meta registry contribution. These counts are over emitted objects/references, not opaque literal contents already separated out.
3. **Compiler shared state.** `SharedContextState::new` reserves the location cache using every object node in the generated JSON (`estimate_subschema_count`). Include all other cache tables, deferred-target deque, pending nodes, alias keys and the deferred in-progress-map copies. Include both initial keyword vectors and final SchemaNode keyword-entry vectors.
4. **Per-keyword factories.** For the initial whitelist the allocation operations are finite and directly visible: type/numeric/count/custom-const are fixed boxed records plus locations; properties allocates either the small tuple Vec with two name copies (PropertyName and FlatJson::PreparedKey) or a large hash map with one name copy; required allocates its names/prepared keys plus specialized 1/2/3-name records and properties+required fusion; allOf allocates its child vector; not clones its original generated child Value in addition to the compiled child. Schedule each branch, not just the generic fallback. The threshold for small versus big properties is 15 in the lock.
5. **Locations and URIs.** Library Location uses Arc<str>; joins allocate joined escaped lengths and may grow a retained thread-local scratch buffer. URI absolute-location construction adds encoded spelling and Arc/URI headers. Count the maximum escaped path lengths and each join/absolute-location call from the recognized factory schedule. Long property names are an independent axis from node count.
6. **Error and returned ownership.** Reserve build-error ownership up to the recognized emitted operands/paths, keyword registrations, retained exact operand records, final Arc<Compiled> and completion checks. No input-sized error clone may occur after exhausting the allowance.

Use a small generated/reviewed layout manifest for private backend record sizes, tied to exact source hashes and supported targets; or conservative mirror-record sizes proven from those fields. SDK-accessible types use `size_of`. A shape/factory with no reviewed schedule returns Semantic. The certificate should expose its components in a test-only record so an allocator failure identifies which schedule underestimated. This is an executable next slice: implement SDK charges and routing first; enable each backend whitelist branch only when its schedule/layout tests and allocator controls pass.

I am explicitly **not** certifying numeric backend constants from inspection alone here. The locked keyword/cache/registry code has not yet received a complete allocation-site manifest. Inventing such constants would recreate the prior report's gap under a larger number. That is the remaining backend proof task, not a reason to keep sending uncertified shapes through the backend while the bounded semantic alternative exists.

## 6. Validation gates

Keep every original expected outcome and performance bound. Retain the prior 86 allocator observations as failing-before evidence. Add the controls in `controls.json` and report route, reservations by stage, actual cumulative requested bytes, cold/warm state, result and post-drop retention separately. Funded semantic-route cases require their original S/F/U; a limit is accepted only in an explicitly low-budget control. Never infer a semantic oracle from a changed route.

Required milestones: (A) tiny/zero caps refuse before projection/literal/origin allocation; (B) heavy paths route before old expansion and keep semantics; (C) each enabled static backend schedule covers all measured allocation requests on native current/MSRV and Wasm layouts; (D) exact/one-below reservations, cancellation at every checkpoint, sticky ignored failures, clean retries and post-build cancellation; (E) current strict core, required evaluator, auxiliary obligations, owner-release/memory and matched performance gates unchanged. A route or schedule expansion requires the relevant controls again, not a permissive oracle rewrite.
