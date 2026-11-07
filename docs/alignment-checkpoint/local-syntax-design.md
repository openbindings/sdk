# Local syntax admission for original schema positions

This is the next design work after prepared operands, not an approved accounting
change. The public API and schema semantics do not change. The original engine
must admit every required structural schema position, including unused definitions,
without charging descendants' entire serialized text at each ancestor.

Current evidence: `fixed_schema::dialect_validity_bounded` reserves 64 times
`value.text().len()` plus fixed overhead for each fixed dialect check. The fixed
2020-12/draft-07/2019-09 meta validators already replace recursive schema edges
with object/Boolean checks; actual child syntax is assessed separately at original
inventory positions. A whole-subtree reservation therefore includes work the
local validator does not perform. `meta_syntax` and `check_from` can also assess
an original position in separate preparation phases. Cold fixed-compiler charges
and actual exact numeric/string/uniqueItems work must still be covered.

Proposed design work, before implementation:

1. Derive the fixed validators' exact read shapes from the vendored meta-schemas
   and their localization transformation. Classify each fixed keyword's own
   scalar constraints, map/array child-type checks, string-array constraints,
   opaque data and equality/uniqueItems work. Do not assume every schema child
   can be skipped: dependencies in all three editions can also be arrays of strings; unknown
   keywords are opaque; default/examples/enum have different constraints.
2. Define a bounded local admission footprint from original node/member ranges
   and the actual read shape. Count child container headers, membership/name
   scans, exact operands and any equality traversal that the fixed validator
   performs. Retain the existing conservative multiplier until this footprint
   and its allocation schedule have independent certificates; do not merely
   replace 64 with a smaller factor or rely on measured RSS.
3. Share completed immutable standard-syntax facts by exact original source/node
   and dialect across admission phases within one preparation. Reuse does not
   mean ignoring footprint limits. No cancelled/partial result is published,
   no remaining-work reset is permitted, and custom meta-schema admission remains
   governed by its actual original meta-schema and aggregate attempt.
4. Preserve strict whole-position admission, even for unused schema positions.
   Ignored draft-07 reference siblings and inactive custom vocabulary positions
   follow their actual inventory/profile rules. Missing meta-schema content and
   invalid supplied schemas retain typed preparation failures, not value effects.

Required controls: deep and wide known schema trees with held-constant local
content; long reference chains; unused malformed subschemas; huge opaque defaults
and enum values; large exact numeric/exponent operands; surrogate names/values in
external resources; dependencies arrays; every supported dialect/custom profile;
cold/warm compilation, low aggregate budgets, every observed cancellation boundary,
clean retry and no partial-certificate reuse. Prove actual local traversal and
allocation bounds with structural controls and a separate allocation witness.
Replay direct original-Plan 1,546 required values and the pinned unchanged default
workloads; no compatibility or inherited message/owner-count assumptions apply.

## Candidate local-footprint mechanism to audit

A bounded initial implementation can retain the existing 64 multiplier and fixed
startup/4096-byte allowances while replacing the charged subtree extent with its
actual local read extent. Begin with the exact original token length. For each
immediate child whose fixed meta edge was localized to an object/Boolean check,
subtract that child's raw token extent and add a constant kind-check unit. These
child extents are disjoint. Keep all member spellings, container separators and
non-schema operands in the footprint. Traversing the immediate headers to compute
it is separately charged before the fixed check; no temporary JSON is built.

The existing standard-dialect selector table is a candidate inventory of those
edges, not sufficient proof by itself. Review it against every localized meta
resource. Direct, map and array schema edges can cut their child payloads;
The dependencies position in all three editions (including the deprecated
2019-09 and 2020-12 root properties) must retain complete array-of-string payloads
and cut only the object/Boolean schema alternative. A wrong-kind containing field must remain conservatively
charged. Keep enum/uniqueItems and all unknown or opaque payload extents intact
until a separate read-shape proof justifies a narrower policy. This intentionally
leaves some over-accounting rather than estimating unproved allocation savings.

Acceptance must demonstrate that no actual recursive/equality/string/numeric read
was removed from the allowance, even where an invalid schema returns early. The
same tests compare original validator verdicts independently from the accounting
helper. A source-sized field is not excluded merely because its keyword sounds
like metadata. No claimed local footprint may silently change if the fixed meta
localization or schema-position inventory changes.

## First implementation boundary

The first increment changes only the per-position charged footprint, guarded by
an audit tied to the exact fixed meta-schema sources and localization behavior.
It does not yet share syntax facts across preparation phases. Existing full
position walks, custom-meta obligations, preparation signatures and the common
attempt remain unchanged. Cross-phase fact sharing in item 3 is subsequent work
with its own ownership, cancellation and cumulative-accounting review.
