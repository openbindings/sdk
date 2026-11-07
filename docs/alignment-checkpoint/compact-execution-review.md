# Compact ordinary execution: independent implementation review

**PASS for the bounded compact-execution increment after the leaf-support correction.** No unresolved finding remains in the reviewed executable change. This does not select Plan as the production default or close iterative traversal, diagnostics, repeated syntax/prepared operands, compiler accounting/removal, full VM preparation, or timing qualification.

Reviewed source is based on efb6cf9, pinned in `final-source-hashes.json`. The copied Plan-only evaluator is byte-identical to the final production semantic sources, except for one explicitly retained private preparation bridge. It links the exact current dependency artifacts selected by Cargo fingerprints; no root manifest, lock, production source, old frozen probe or baseline binary was modified. Build commands, dependency hashes, bridge source and final executable hashes are retained.

## Findings and resolution

**R-CE1, design hazard — resolved.** The implemented leaf certificate checks keyword presence before malformed/orphan child filtering. All active body child-call keywords are covered by `child_shape`, plus foreign `dependencies`/`additionalItems`; every active reference disqualifies a leaf. Empty custom anyOf/oneOf remain F rather than becoming a primitive-only S. All 18 six-kind empty-applicator controls pass through the independent direct Plan bridge.

**R-CE2, required failure lost for a foreign leaf — resolved.** Parent identified the candidate; independent public reproduction confirmed it. Root `allOf:[ref A]` plus `unevaluatedProperties:false`, with A containing a self-reference and an embedded 2019 `type:object` leaf, must fail `{a:1}`: no known applicator can originate that evaluated property, regardless of the recursive validity result. The pre-correction source returned UnsupportedCapability/semantic-analysis-inconclusive. The unchanged frozen older Plan returned F, while replacing the leaf's profile with 2020 also returned F in the candidate. Exact input, failed observation and old-binary result remain in the pre-correction files here.

Removing leaf memo entries had removed the proof that let support discovery bypass its standard-2020 restriction. The correction uses the preparation-certified leaf as a structural zero-producer certificate before that restriction. This is sound for every context because the leaf has no active applicator/reference, and does not infer absence from an omitted annotation map. Final 2019, custom, custom-with-inactive-consumer, dynamic-anchor and false-leaf variants all return the required F, with cancellation/retry controls.

## Ownership, effects and demand audit

Each Plan::at computes its own flags and leaf set after profile/eligibility/inventory checks. Existing known dynamic/recursive closure is preserved. Runtime leaves retain work/depth checks and resource entry, execute the same primitive/format/pattern helpers and skip only child traversal and Visit/active/memo retention. Original effect keys and exact value interning are unchanged. Non-leaf memo, active occurrence/scope identity, cut namespaces, pending-fact isolation and retry/no-progress rules remain present.

Property/item producers and unknown-schema coordinate effects are dimension-gated. Recursive cuts skip unsupported dimensions before support lookup, and coordinate certification never reads an omitted map as FALSE. The full demanded-map path remains in use when its bit is on. Local explicit/pattern property selection, contains counts and legacy tuple selection remain active without outward maps. An additionalProperties-only object no longer allocates exact name copies solely for unused annotation coordinates. No existing allocation multiplier or default limit was reduced, including the conservative empty-context charges.

The known demand closure remains closed and immutable. No public operation subsequently fetches content or activates a profile within a retained Plan. A private future extension that does so would need to rebuild these certificates. Partial/cancelled construction is not published, and typed inability is not relabeled as Undefined.

## Independent results

The fresh private bridge constructs only `semantic::Plan` and returns a validator with no compiled fast path. It preserves original schema/document/resource contexts rather than changing dialects to force execution.

- **1,546/1,546 required kit cases pass**, with zero preparation refusals, case failures or configuration failures, before and after the narrow support correction. Outcome and explanation consistency are checked by the existing required harness. Full original observations remain in each `semantic-required-suite.json`; compact summaries are adjacent.
- **57/57 expanded public controls pass** after correction. They cover all 25 design control IDs (empty-custom records expand over six kinds), the three exact frozen jobs, local selection/counts, demanded scopes and failure gating, unknown content, distinct/repeated invalid leaves, exact names, dynamically selected consumers, and depth-20 shared non-leaf diamonds under 4,000 work units.
- **1,206 observed evaluation cancellation checkpoints** across compact arrays, demanded objects, uncertain leaves, foreign/custom support, dynamic consumers and shared diamonds all return Cancelled. Clean retries succeed. Prepared results remain usable after dropping the originating document, proof, selected schema, external set and evaluator.
- Low program allowance returns LimitExceeded, not a semantic verdict. Both Outcome and Explain modes retain S/F/U/typed inability as required; these checks do not assert the still-open first-problem diagnostic contract.

The `known-name-exact` design row did not state its embedding. The initial adapter placed its lone-surrogate schema key inside the OBI and correctly encountered existing OBI-01 `unpaired-surrogate` admission rejection. That original negative document/report is preserved in `lone-contained-*`. The same exact schema/value bytes are then supplied as an external schema, where the intended exact-name evaluation control is meaningful and returns S. No semantic expectation or production admission rule was changed.

## Unchanged workload restoration and measured allocation

`ordinary-restoration-workloads.json` preserves the frozen source file hash and exact original schemas/value strings. No simplified invalid array replaced the frozen late-invalid case.

| Original job | Required/final result | Requested evaluation bytes | Incremental requested-heap peak |
| --- | --- | ---: | ---: |
| 20,000 integer array | S | 780 | 556 |
| 20,001 items, last invalid | F | 821 | 597 |
| 50,000 integer properties | S | 780 | 556 |

These use unchanged default limits and actual direct Plan execution. Measurements use the existing requested-System-heap counter around evaluation after preparation/value parsing. S calls end at the same live baseline; the F observation retains its 41-byte returned failure message at measurement end. These are allocation observations, not a timing campaign or a whole-application RSS measurement. Prepared/source/value ownership is included in the reported baseline, not hidden from it.

Parent separately reports final current/MSRV 267 evaluator tests, required corpus, strict lint, Node/package/type and both browser core/evaluator gates. Those broader lanes are not presented as independently rerun by this reviewer. The independently observed restoration and correctness results authorize this bounded checkpoint, while the larger original-engine replacement gates remain open.
