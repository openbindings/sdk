# OpenBindings schema comparison

Compare schemas by normalized identity or directional compatibility under profile
`OB-2020-12` version `0.1`, pinned at interfaces revision
`67f707c31aecbdf0551385e369d8024097db6278`. This unpublished companion is independent
of core document conformance and the value evaluator. It implements the profile's
directional rules and interface-level fold, plus the source puller's reference
rule for existing-entry identity.

```rust
use openbindings::JsonValue;
use openbindings_schema_comparison::{identical, ComparisonLimits, SchemaInput};

let offered = JsonValue::parse(
    r#"{"type":"string","nullable":true,"description":"offered value"}"#,
)?;
let offered = SchemaInput::new(offered.clone(), offered);
let existing = JsonValue::parse(r#"{"type":["null","string"]}"#)?;
let existing = SchemaInput::new(existing.clone(), existing);
assert!(!identical(Some(&offered), Some(&existing), &ComparisonLimits::default())?);
assert!(!identical(None, Some(&existing), &ComparisonLimits::default())?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`SchemaInput::new` retains a selected exact schema and its document root. The root
need not be an OBI: a source puller can compare an offered schema before authoring
it. `SchemaInput::from(&schema)` retains a canonical core schema's original context.
Dropping the original document or view does not invalidate the input. Absent slots
remain `None`; present null is an invalid schema. Every present side normalizes
before an identity verdict, including when the other side is absent.
`identical` returns `Result<bool, ComparisonFailure>`: true means normalized
identity, false means difference, and a failure gives no answer.

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
and comparison. Conjunction checks index closed branches and branches declaring
properties, charge every remaining pair check and stop once retention is known.
Conjunctions of empty objects need only linear work in their branch count.
Default limits are 20 million work units, 64 MiB of conservative
cumulative allocation reservations and depth 128. Depth can be tightened but the
fixed ceiling remains 128. Work units and reservations are SDK policy, not a
portable cost metric or a bound on caller-owned input storage. A refused attempt
stays refused; fresh retries use unchanged inputs. `identical_with_control` observes
cooperative cancellation at work and allocation boundaries. It never publishes a
partial normalized tree or caches a failed result.

The fixture harness passes all 27 ordinary and 10 exact identity groups, and all
59 ordinary and 29 exact directional groups, including expected normalization
failures. No directional groups are skipped. Copied inputs retain the pinned
publication's JSON values, with hashes in `tests/fixtures/README.md`. Two fixture
descriptions encode U+2014 with JSON escapes; the other pinned files retain the
publication's bytes. Two manifest
finding-label sets for union cross-form cases conflict with the prose's rule
order: string bounds decide before unions. The harness pins those prose-governed
findings explicitly while checking every published verdict.


## Directional comparison and interface checking

`compare(Direction::Input | Direction::Output, target, candidate, limits)` accepts
two present `SchemaInput`s. `compare_with_control` adds cooperative cancellation.
Both return `Result<Comparison, ComparisonFailure>`. `Comparison::verdict()` is
`Compatible`, `Incompatible`, or `Indeterminate`; `reason()` exposes report data
and `finding()` returns `outside-profile` for indeterminate positions. Public
`Debug` and diagnostic `Display` never disclose schema values or reason strings.
Normalization failures, including unsatisfiable merges and resource refusals, are
errors without a verdict. Ordinary `identical` retains its behavior and work path.

```rust
use openbindings::JsonValue;
use openbindings_schema_comparison::{compare, ComparisonLimits, Direction, SchemaInput, Verdict};
let target = JsonValue::parse(r#"{"type":"integer","minimum":1}"#)?;
let target = SchemaInput::new(target.clone(), target);
let candidate = JsonValue::parse(r#"{"type":"number","minimum":0}"#)?;
let candidate = SchemaInput::new(candidate.clone(), candidate);
let result = compare(Direction::Input, &target, &candidate, &ComparisonLimits::default())?;
assert_eq!(result.verdict(), Verdict::Compatible);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The walk applies identity, outside-profile refusal, Top, type, const/enum,
objects, items, numeric bounds, string bounds, array bounds and unions in that
order. It stops at the first failing rule. Identity prunes even outside-profile
positions. Retained conjunctions with no directional merge rule fail closed when
they differ. Union search accepts a definitive matching variant; if no variant
matches and some comparisons are indeterminate, that variant remains indeterminate.
Reasons retain nested paths, exact JSON values, exclusive markers, actual union
keys and authored indexes. Types, properties and required names choose the first
lexical failure; enum and union traversal retain authored order.

`check_interface(required, provided, limits)` accepts two `ConformantDocument`s.
Swap these explicit roles to check consumption in the other direction. It matches
a required operation's key, then its aliases, in the provided flat key-and-alias
namespace. It keeps every required operation in sorted key order, including
missing ones, and checks output before input. A slot with either schema absent
is skipped without normalization. `operations()` retains slot results and
per-operation failures; `issues()` exposes missing, incompatible, indeterminate
and failed checks in report order. `verdict()` returns the first comparison
failure if one exists; otherwise it uses `indeterminate > incompatible > compatible`.
`Verdict::collapse` supplies that dominance for caller-owned verdict collections.

`check_interface_with_control` adds cooperative cancellation. One allowance covers
the complete fold. Setup failure returns no report; after setup, a failed slot
stays in the report and later checks continue. Exhaustion is sticky, so subsequent
present pairs retain the same resource failure. An added provided operation has
no required check. This API does not choose report suppression policies.

## Source puller's reference rule

`identical_with_references(left, right, accounted_for, limits)` compares optional
slots using the source-puller rule. The callback receives a `ReferenceContext`
with decoded spelling, the original schema, its resource root and base, and
decides accounting; the companion never reads a pull report.
References spelled the same on both sides and accepted by the callback remain
leaves. Their siblings normalize, but stay unmerged, as does an allOf containing
such a leaf. Other references are followed. All other identity normalization,
exact equality and failure rules apply. `identical_with_references_and_control`
adds cooperative cancellation; each call uses one monotone budget, including
reference inventory and comparison work.

```rust
use openbindings::JsonValue;
use openbindings_schema_comparison::{identical_with_references, ComparisonLimits, SchemaInput};
let schema = JsonValue::parse(r##"{"$ref":"#/schemas/User"}"##)?;
let before = SchemaInput::new(schema.clone(), JsonValue::parse(r#"{"schemas":{"User":{"type":"string"}}}"#)?);
let offered = SchemaInput::new(schema, JsonValue::parse(r#"{"schemas":{"User":{"type":"number"}}}"#)?);
let limits = ComparisonLimits::default();
// The caller has a report entry for User, so its drift is reported there.
assert!(identical_with_references(Some(&before), Some(&offered),
    &|reference| reference.spelling() == "#/schemas/User", &limits)?);
// Without that accounting, following the reference finds a difference.
assert!(!identical_with_references(Some(&before), Some(&offered), &|_| false, &limits)?);
// Keeping every same-spelled reference answers the second question: the entry
// itself is unchanged, so the difference is only behind an unaccounted target.
assert!(identical_with_references(Some(&before), Some(&offered), &|_| true, &limits)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

A source puller reports the last situation as `unassessed`. Annotation-only edits
remain `current`. Binding and source data still compare as exact JSON values.
