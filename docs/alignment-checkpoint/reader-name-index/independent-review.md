# Ordered Reader names: bounded implementation review

**PASS for this private representation correction.** Production source reviewed:
`crates/openbindings-json-schema-evaluator/src/pattern.rs`, SHA-256
`2655aa2c2e47870995809cd395b19ac4802c4676ad297de0eabc1e639692b632`,
against checkpoint `e76ca515dd4351b46bdee79a52f622ec9ed952a4`.
No SDK files were changed by this review. Whole Reader resource certification and
full compiler activation are outside this result.

The diff replaces `Groups.names: Vec<String>` with `BTreeSet<String>` and folds
membership-plus-push into insertion's duplicate boolean. The only other production
change is the import. Existing named-reference `contains` now uses the set.
Capture count and source-order traversal are unchanged; names are never iterated to
assign capture numbers or produce normalized output. Comparison is on the decoded
String, with no new case folding, Unicode normalization or source-spelling equality.
It preserves forward names, exact escaped-name identity, malformed-name handling
and the scan's existing invalid-before-unsupported precedence. An empty BTreeSet
allocates nothing. Insertion-only storage gives finite ordered lookup rather than
repeated whole-name-vector scans; arbitrary string comparison still costs its
shared prefix and is not constant time.

Independent executions used the copied actual source probes under Rust1.99 and
Rust1.88. The copied module was verified to begin with the exact production bytes;
its only appended code observes the private Groups scan. Every copied support
source and binary was checked against its recorded manifest. The old vector probe
is separately preserved and hashed. This review reran the binaries on its own
inputs; it did not merely accept the parent's summarized results.

- All46 retained name-specific cases and1,186 unchanged retained rows match the
  old full Reading exactly on both versions, including normalized forms and Simple
  data. No observed request exceeds the existing Reader preparation reservation.
- All59 new independent controls pass both versions: zero/one/split-boundary and
 10,000 sequential names; forward and numerical references; decoded escaped
 duplicates; malformed-before-unsupported; case distinction; canonically related
 but unequal Unicode strings; astral escape equality; two1,024-name orderings with
 512-character shared prefixes; and explicit no-name cases.
- Every measured Reader and Groups allocation is released. All explicit no-name
  controls request zero Groups storage. Maximum requested bytes in the59-input
  lane:8,512,652; maximum live peak:3,773,056; maximum isolated Groups requests:
 2,139,240. Current/minimum Rust observations are identical for this lane.

The set does allocate tree nodes for named patterns; its speed improvement is not
a claim of zero additional storage. The unchanged preflight reservations cover all
these measured inputs, including10,000 names and the long common-prefix cases.
The current compiler-policy correction must retain early Reader expansion and
class-pressure protection; this patch neither removes it nor establishes a new
whole-Reader operation/allocator guarantee. The earlier independent comparison
instrumentation's improvement (1,048,576→35,314 comparisons for its worst1,024
case) is useful evidence, not the semantic oracle and not a universal timing ratio.

Permanent added source tests cover escaped duplicate identity, duplicate-before-
unsupported, unknown/forward/numeric/Unicode references, and source-order
normalization with1,024 names. The independent cases supply NFC distinction,
astral identity and larger set growth beyond that compact permanent set.

Artifacts: `inputs.jsonl`, `run.py`, `summary.json`, `replay-summary.json`, all
current/minimum/vector raw observations and binary hashes. The parent's native
library and actual Wasm/host gates are separate corroborating evidence; this
bounded review does not claim to have independently rerun those whole-package
host gates.
