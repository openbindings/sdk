# OpenBindings schema comparison

Compare optional schemas by normalized identity under comparison profile
`OB-2020-12` version `0.1`, pinned at interfaces revision
`5006884a39c055103fad49e7c47b3141726f8dd8`. This unpublished companion is independent
of core document conformance and the value evaluator. Directional compatibility
and whole-profile conformance are outside this initial API's claim.

```rust
use openbindings::JsonValue;
use openbindings_schema_comparison::{identical, ComparisonLimits, Identity, SchemaInput};

let offered = SchemaInput::standalone(JsonValue::parse(
    r#"{"type":"string","nullable":true,"description":"offered value"}"#,
)?);
let existing = SchemaInput::standalone(JsonValue::parse(
    r#"{"type":["null","string"]}"#,
)?);
assert_eq!(identical(Some(&offered), Some(&existing), &ComparisonLimits::default())?, Identity::Different);
assert_eq!(identical(None, Some(&existing), &ComparisonLimits::default())?, Identity::Different);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`SchemaInput::new` retains a selected exact schema and its document root. The root
need not be an OBI: a source puller can compare an offered schema before authoring
it. `SchemaInput::from(&schema)` retains a canonical core schema's original context.
Dropping the original document or view does not invalidate the input. Absent slots
remain `None`; present null is an invalid schema. Every present side normalizes
before an identity verdict, including when the other side is absent.

References resolve within their containing schema resource, including nested `$id`
resources and plain-name anchors. Without `$id`, fragments use the supplied document
root. Relative references need a resource base; external content must be supplied
with `with_resource`. Nothing is fetched. A `$ref` target is conjoined with its
siblings using the profile's `allOf` rules. Missing content, cycles and malformed
references fail normalization. Duplicate supplied URI identities are refused.

Normalization strips the profile's annotations/extensions, canonicalizes type and
required, resolves references, and merges eligible allOf branches in authored
order. Outside-profile keywords and their contents remain exact opaque data.
An allOf with an outside-profile sibling or branch, a union, or an additionalProperties
constraint that merging would loosen remains ordered. `nullable` stays outside the
profile, and `$defs` remains where retained keywords in its resource could depend
on it. Union arrays at actual schema oneOf/anyOf
positions compare as multisets with multiplicity; ordinary arrays, including
const/enum and unknown-keyword contents, remain ordered. Literal additionalProperties
booleans retain their flags; object forms normalize as schemas. The profile's
literal false/true merge rules do not rewrite object forms into those flags.

Numbers compare mathematically without binary64 conversion or exponent expansion.
Names and strings retain decoded UTF-16 identity without Unicode normalization.
Ambiguous duplicate object members are refused, including inside opaque data.
Enum intersections retain the first branch's order; repeated equal first-branch
values currently return UnsupportedNormalization because the pin does not specify
their canonical intersection multiplicity. Standalone enum order/multiplicity is
preserved. Empty type/enum intersections and conflicting const values fail the
merge rather than producing an identity answer. Other cross-keyword constraints
remain separate fields as the table prescribes: merging const with enum or lower
with upper bounds does not invoke a general satisfiability solver. An identical
merged object can still admit no values; identity is a structural profile claim.

One monotone attempt covers both sides, reference lookup, normalization, merges
and comparison. Default limits are 20 million work units, 64 MiB of conservative
cumulative allocation reservations and depth 128. Depth can be tightened but the
fixed ceiling remains 128. Work units and reservations are SDK policy, not a
portable cost metric or a bound on caller-owned input storage. A refused attempt
stays refused; fresh retries use unchanged inputs. `identical_with_control` observes
cooperative cancellation at work and allocation boundaries. It never publishes a
partial normalized tree or caches a failed result.

The preserved fixture harness passes all 27 ordinary and 10 exact identity groups; the 59 ordinary and 29
exact directional groups are explicitly outside scope rather than counted as passes.
