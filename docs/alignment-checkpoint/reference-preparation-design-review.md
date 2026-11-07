# Independent review: repeated-reference preparation

**Design gate: PASS for the bounded increment, with the implementation invariants below. This is not an implementation or OE-05 completion claim.** Reviewed SDK source is `98ab4753f7f78ae1d5f381717477d2b6dd83d26d`; exact source hashes are in `source-hashes.json`. Production was not edited. The subsequent implementation must be reviewed separately.

The proposed changes remove two repeated reads without changing which original schemas are admitted or what their references mean. The two-vector storage refinement and the 1,024-key optimization cap are appropriate. A private borrowed ledger adapter is sufficient; no public abstraction, parallel registry, or preparation Graph is needed.

## Authority and evidence

The pinned specification is `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`. Its §5.5 fixes dialect/evaluation and dependence on undefined or external results; §§7.2–7.4 fix original resource boundaries, exact contained identity precedence, fragments and dynamic resolution. This change must preserve those outcomes and original locations. The local fixed-meta accounting policy is an SDK implementation certificate, not a new specification rule.

I inspected the exact original query implementation, fixed-meta resources/root selection/options, locked `jsonschema`/`jsonschema-value` 0.58.6 type validators, internal exact-JSON string access, preparation traversal and the already reviewed Path backing schedule. `audit.py` verifies six constrained reference-keyword/edition pairs against the complete pinned localized snapshot and emits 210 proposed read-shape controls. These 210 controls are implementation acceptance inputs, **not claims that the unimplemented optimization has passed**.

Nineteen separately reasoned semantic controls were replayed against the existing copied-source direct-Plan bridge: **19/19 pass in both Outcome and Explain**. They establish a baseline; they do not run the new cache. Exact inputs, answers, observations and script are retained. In particular, six controls use the same initial target for `$ref` and `$dynamicRef` in both orders while an outer dynamic candidate lives in an otherwise unvisited definition.

## 1. Fixed syntax read extent

The complete localized meta graph has these exact type/format constraints:

| Edition | Field | Route to terminal constraint |
| --- | --- | --- |
| Draft 7 | `$ref` | root `properties.$ref` |
| 2019-09 | `$ref`, `$recursiveRef` | core `properties` |
| 2020-12 | `$ref`, `$dynamicRef` | core `#/$defs/uriReferenceString` |
| 2020-12 | legacy `$recursiveRef` | root → `meta/core#/$defs/uriReferenceString` |

The terminal constraint is `type: string` plus `format: uri-reference`. The fixed compiler explicitly disables asserting formats. Modern StringTypeValidator calls `is_string`, whose default calls `json_type`; the exact-JSON adapter checks the node kind without materializing the string. Therefore a qualifying string's contents need not be included in this fixed local check's multiplied read extent. Keep a token header and charge the field/header discovery work.

There is one important precision correction to the draft's “type-only” wording: Draft 7's reference-object path short-circuits to `as_str().is_some()`. This is an O(1) arena-metadata/scalar-presence check, not a decode/scan, but it currently rejects a string containing an unpaired surrogate. Preserve that admission behavior. The optimization does not authorize replacing the shortcut with a broader kind check.

The 2020 legacy `$recursiveRef` constraint is active **for syntax**, despite that keyword being inactive in 2020 evaluation. Select this accounting table by the actual fixed meta graph, not by the evaluator's keyword activation mask. A smaller implementation that initially optimizes only `$ref` in all three editions would also be sound, but should disclose its narrower coverage.

Keep every wrong-kind operand, unknown/unsupported keyword, `$id`, anchor, pattern and other text-sensitive operand at its full current extent. Keep dependency arrays and other previously reviewed child-cut rules unchanged. A reference token is disjoint from the immediate child-schema cuts, so subtract its exact original token extent once, with checked arithmetic, then add one header. The validator receives the complete original value. This does not validate a URI or alter its result.

The complete resources **and selected entry root** snapshot remains essential. In addition to that graph certificate, retain a source-tied certificate for the fixed compiler options and actual type-validator/adapter read path. A graph-only snapshot cannot detect `should_validate_formats(true)` or a backend switch from kind checking to text presentation. Required negatives are an added pattern/enum/const on the field, an ancestor equality constraint, a selected-root change, and an asserting-format mutation. Long ASCII, private-use characters, escaped tokens and lone-surrogate controls must detect unexpected scans/materialization while preserving existing admission answers. Actual selected custom meta-schema evaluation remains fully accounted; this shortcut never applies to it.

## 2. Static query equivalence

Within **one completed immutable SchemaSpace**, this key is sufficient for the reviewed source:

`(original resource identity, original.profile.is_some(), exact decoded scalar reference str)`

“Exact” here means the JSON string's decoded value, not the raw JSON token. `"https:\u002f..."` and the corresponding literal-slash JSON token may share when they decode equally. Do not normalize URI case, ports, percent escapes or dot segments in the key. Distinct resources with equal base strings remain distinct keys.

The proof is tied to `space/original.rs`:

1. `query` checks the reference's URI syntax.
2. `query_identifier_interpretation(holder)` reads only the holder's profile-presence flag plus its resource's immutable dialect.
3. `query` reads the holder's resource for eligibility, anonymous status and compared base.
4. `named_resource` reads the immutable name policy, contained precedence, conflict and namespace-uncertainty state.
5. `query_fragment` and `query_pointer` read the selected carrier, original source/node inventory and target resource guards. They do not use the referring node.

Thus no original node index is necessary in this temporary key today. Adding future holder-sensitive reads must reopen this certificate. Keeping the presence bit is inexpensive and conservative even where current traversal already excludes stopped nodes. There is no cross-space cache, so resource IDs cannot escape their identity domain. Borrowed key text lives only for this construction call and is retained by its original environment owner.

Share **only `Arc<Result<Target, QueryFailure>>`**. The existing permanent map remains keyed by original node plus ReferenceKind. Neither diagnostics with a referring origin, semantic effects, instance coordinates, dynamic scope nor completed evaluation state may be shared through this cache. `Target.dynamic_name` is initial-resolution evidence; effective dynamic/recursive dispatch remains per evaluation.

A hit replaces only the resolver call/Arc construction. It must still perform all per-occurrence work following that call:

- insert the original node/keyword's prepared query entry;
- queue the static target;
- process `$dynamicRef`/`$recursiveRef` candidate discovery and the `used` names, even when a preceding `$ref` filled the cache;
- account for distinct reached/missing resources;
- produce failures using the current referring node's original source/pointer.

The miss reservation currently covers URI-resolution string work that also incidentally covers some downstream work. Removing it on hits requires explicit, size-proportional charges before missing-URI hashing/cloning, dynamic-name hashing/comparison/cloning and recursive-scope insertion. Preserve typed failure contents; do not silently truncate a URI to meet a charge. The six static/dynamic controls are specifically designed to fail if hit processing bypasses candidate discovery.

## 3. Bounded storage and ledger adaptation

Reuse the existing Path representation: ordered entries with stored hash and prior bucket index, plus bucket-head vector. The query table uses it insertion-only. Execution active/alias paths retain their existing LIFO behavior. Do not add a new standard HashMap layout multiplier.

A minimal private ledger interface should expose work charging, requested-allocation charging and typed allocation exhaustion. Path owns no ledger or borrowed control. Each operation receives a temporary adapter: Graph maps to evaluation NoVerdict; preparation maps to its existing Control/Failure and shared Attempt. No adapter resets a work scope or grants a new allowance.

Every checked size/capacity overflow and failed allocation must use the adapter's exhaustion hook. The preparation adapter must call `allocation_limit_reached` so an ignored error cannot yield Ready. Preserve the prior cancellation/failure rather than overwriting it with a later overflow. Work arithmetic overflow is a work refusal; allocation arithmetic overflow is an allocation refusal. These are operational failures, never schema defects or uncertainty variables.

Retain the already reviewed full-new-backing reservation before `try_reserve_exact`, including relocation overlap; charge rehash operations and hash/equality scans. Hash matches require full key equality. A collision is never identity. Work for a borrowed key includes its string length plus fixed key fields; collision comparison work must be charged even when it does not find a match. The repeated 1,024-byte spelling is still read on each lookup.

**Cap policy:** search existing entries first. At 1,024 distinct retained keys, a new miss performs the original fully reserved query and fills its original-node prepared entry, but does not extend the temporary cache. Existing hits remain usable. Cap exhaustion is neither a refusal nor permission to skip resolution. No eviction is required. Avoid eagerly allocating the full cap.

For the current doubling schedule, at most 1,024 insertion-only entries need 2,048 bucket heads. Cumulative requested backing capacities are at most `2,044 * size_of::<Entry<K,V>>() + 4,088 * size_of::<usize>()`; retained backing is at most `1,024 * EntrySize + 2,048 * usizeSize`, exclusive of already charged result owners and the existing permanent map. Derive sizes on the target, rather than copying a 64-bit number into Wasm. Actual allocator overhead/peak remains a measured lane, distinct from this requested-byte policy.

A table-growth refusal after a result is resolved still aborts the shared construction attempt. Only the explicit insertion cap is a no-allocation optimization fallback. Nothing from a failed/cancelled attempt becomes a completed Plan. Dropping the temporary table releases its extra Arc references; permanent per-occurrence entries retain their original result owners.

## 4. Acceptance gates

Before claiming implementation completion:

1. Replay all 210 read-shape controls against the real extent helper and actual bounded checker, including cold/warm paths. Assert unchanged admission and the precise eligible-token extent changes. Run the graph/root/options/backend negative controls described above.
2. Replay the 19 semantic inputs verbatim through forced original Plan, in both detail modes. Preserve all original 1,546 required cases, existing recursive/annotation/exact-string controls and unchanged large/depth jobs.
3. Add private query-key equivalence controls for same text under distinct resources/bases, anonymous versus named carriers, stopped profile-presence states, ambiguous IDs/anchors, non-schema pointers, exact contained versus normalized supplied aliases and source-record replacement. Compare static Target/typed failure to uncached `query` for each original holder, not merely the final verdict.
4. At one node exercise static then dynamic, dynamic then static, and 2019 static/recursive initial-resolution sharing. Assert candidate discovery/resource count and original per-keyword entries. Preserve referring origins for conflicts and unavailable content; do not use the cache key as a diagnostic location.
5. Exercise 1,023/1,024/1,025 distinct keys, a late hit after the cap and a repeated uncached miss. Assert bounded temporary entry count, normal fallback, all original entries and unchanged answers. Add forced hash collisions, equal-prefix strings, rehash and target-width overflow controls to both ledger adapters.
6. Enumerate cancellation at all observed preparation boundaries on hit/miss/growth/cap fallback, then retry with a fresh control. Test ignored tighter nested allocation failures, exact/one-below shared work and allocation limits, and failed allocation/nonpublication. No warm artifact may retain the construction control/table.
7. Replay the frozen OE-05 **1,000/5,000 sibling-reference** inputs at original defaults, preserving the existing preparation-refusal baseline. Measure total preparation work across environment construction, fixed checks, temporary lookups, downstream sets and permanent map insertion. One phase's savings do not prove whole-job completion. Report requested bytes, actual peak, retained bytes and timings separately.
8. Keep native current/minimum Rust, actual Wasm/browser, retained-owner concurrency and the existing resource/performance gates. Cache-cap policy does not waive any of these gates.

The source-sharing and read-shape changes can be implemented as one bounded increment. Cross-phase syntax-fact sharing, full pattern VM-once preparation, diagnostics and the whole original-engine performance/switch gates remain separate and open. No public API compatibility constraint is introduced.
