# OpenBindings schema comparison

Compare schemas by normalized identity or directional compatibility under profile
`OB-2020-12` version `0.1`, pinned at interfaces revision
`a3c19658fc89815d7a417eae29ba31fd4e930daf`. This unpublished companion is independent
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
Dropping the original document or view does not invalidate the input.
For many comparisons, construct one `ComparisonContext::new(root)` per document
and select inputs with `context.schema(exact_schema)`. Its `with_base` and
`with_resource` configure resolution, just as the convenience input methods do.
Contexts retain completed successful indexing across calls. Failed or cancelled
work leaves them usable. The persistent index contains only configured documents
and resources, never detached selections. An OBI contributes declarations only
at its schema positions; source and binding data remain data. A non-OBI root is
treated as one JSON Schema document, following recognized schema edges from that
root, just as the evaluator indexes a supplied resource document. Opaque carrier
members do not contribute declarations merely because a caller selected them.
The profile does not prescribe a declaration inventory for arbitrary non-OBI
carriers; this fixed-root rule keeps resolution independent of selection history.
A schema selected outside the root's schema positions, including an opaque
carrier member, gets the same call-local view of its subtree's declarations as
a detached schema. These declarations are private to that view: sibling
selections and the other side of a comparison cannot see them, and the view is
released with the call. Pointer references still resolve into the configured
document unless the selected subtree declares its own resource with `$id`.
Selecting carrier data adds no declarations to the containing document.
Within a call, inputs with the same root and resource
configuration share indexing, including reference inventory and normalization.
Only reached schemas are normalized; unused defects and deep unused schemas do
not fail an unrelated comparison. A resource or anchor lookup discovers identifiers
lazily, without validating unrelated content. Absent slots
remain `None`; present null is an invalid schema. Every present side normalizes
before an identity verdict, including when the other side is absent.
`identical` returns `Result<bool, ComparisonFailure>`: true means normalized
identity, false means difference, and a failure gives no answer.

`ComparisonFailure` is a struct: `kind()` returns the Copy
`ComparisonFailureKind`, `origin()` borrows an optional original `SchemaOrigin`,
and `side()` returns an optional `ComparisonSide`. Identity calls use `Left` and
`Right`; directional calls use `Target` and `Candidate`; interface failures use
`Required` and `Provided` even for consumed roles. Match on `kind()` to classify
failures. Whole-error equality also compares provenance. Cloning a failure shares
its immutable location; `InterfaceIssue::failure()` preserves that location.

Locations point into original documents or supplied resource roots, including
supplied subtrees, rather than normalized schemas. A nested `$id` never replaces
the supplied retrieval identity. One known schema coordinate is retained for a
merge involving multiple sources. Unknown ancestry leaves the origin absent;
global allowance exhaustion and cancellation also have no operand side. Location
storage is bounded by the attempt's allocation allowance, retained across slots,
and limited to an 8 MiB exact pointer. A pointer exceeding that cap is omitted;
an exhausted attempt allowance returns `LimitExceeded`. Debug and Display never
include supplied values; inspect `origin()` explicitly for editor diagnostics.

References resolve within their containing schema resource, including nested `$id`
resources and plain-name anchors. Without `$id`, fragments use the supplied document
root. Relative references need a resource base; external content must be supplied
with `with_resource`. Nothing is fetched. A `$ref` target is conjoined with its
siblings using the profile's `allOf` rules. Missing content, cycles and malformed
references fail normalization. Duplicate supplied URI identities are refused.

Normalization strips the profile's annotations/extensions, canonicalizes type and
required, resolves references, and merges eligible allOf branches in authored
order. Outside-profile keywords remain authored data. References at schema positions
inside retained 2020-12 applicators, and initial `$dynamicRef` targets, must resolve
and have identical normalized targets for identity. They remain uninlined.
Missing targets and cycles fail, including when the authored content is equal.
A `$ref` inside a non-applicator keyword is data.
An allOf with an outside-profile sibling or branch, a union, or an additionalProperties
constraint that merging would loosen remains ordered. `nullable` stays outside the
profile. `$defs` remains only on a schema whose own schema subtree carries a
retained keyword. Unrelated schemas and non-schema carrier members cannot keep it. Union arrays at actual schema oneOf/anyOf
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

One shared attempt covers both sides, reference lookup, normalization, merges
and comparison. Conjunction checks index closed branches and branches declaring
properties, charge every remaining pair check and stop once retention is known.
Conjunctions of empty objects need only linear work in their branch count.
Work is uncapped by default (`max_work: None`). A time bound is the caller's
choice, through `WorkControl` cancellation or `Some(work_units)`. Memory keeps
a default 64 MiB conservative allowance for storage held at once, and depth 128.
Slot scratch is released after each check; reports, persistent reference indexes
and shared normalized targets stay charged. Depth can be tightened but the
fixed ceiling remains 128. Work units and reservations are SDK policy, not a
portable cost metric or a bound on caller-owned input storage. A refused attempt
stays refused; fresh retries use unchanged inputs. `identical_with_control` observes
cooperative cancellation at work and allocation boundaries. It never publishes a
partial normalized tree or caches a failed result. Completed document indexing
can be reused without charging that indexing again in a later call.

The fixture harness passes all 30 ordinary and 10 exact identity groups, and all
62 ordinary and 29 exact directional groups, including expected normalization
failures. No groups are skipped. All copied inputs are byte-identical to the pin,
with hashes in `tests/fixtures/README.md`.

## Directional comparison and interface checking

`compare(Direction::Input | Direction::Output, target, candidate, limits)` accepts
two present `SchemaInput`s. `compare_with_control` adds cooperative cancellation.
Both return `Result<Comparison, ComparisonFailure>`. `Comparison::verdict()` is
`Compatible`, `Incompatible`, or `Indeterminate`; `reason()` exposes report data
and `finding()` returns `outside-profile` for indeterminate positions. Public
`Debug` and diagnostic `Display` never disclose schema values or reason strings.
Normalization failures, including unsatisfiable merges and resource refusals, are
errors without a verdict. Ordinary `identical` changes only where the amended profile changes identity:
retained reference targets participate, and `$defs` retention is schema-local.

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
positions. A non-identical position carrying both `oneOf` and `anyOf` is
indeterminate before any other directional rule runs. Retained conjunctions with no directional merge rule fail closed when
they differ. Union search accepts a definitive matching variant; if no variant
matches and some comparisons are indeterminate, that variant remains indeterminate.
A variant with no match decides incompatible before an indeterminate variant
decides indeterminate, whatever order the variants were written in; the reason
names the first authored variant of the deciding kind.
Reasons retain nested paths, exact JSON values, exclusive markers, actual union
keys and authored indexes. Types, properties and required names choose the first
lexical failure; enum and union traversal retain authored order.

`check_interface(required, provided, required_context, provided_context, role, limits)`
accepts two `ConformantDocument`s, their caller-configured `ComparisonContext`s and a callback
`&dyn Fn(&Operation, Option<&Operation>) -> Option<OperationRole>`.
Each context must retain the exact root of its paired proof, obtained from
`document.document().value().clone()`. An equal but separately parsed root fails
with `ComparisonFailureKind::ContextMismatch` before any callback runs. Initial
cancellation takes precedence. Retrieval bases and supplied resources remain
independent on the two sides, and no reference causes I/O.

The callback receives the required operation and each distinct matched provided
operation, or `None` when missing. It runs once per match, before expanding `Both`.
Return `Realized`, `Consumed`, `Both`, or `None` to exclude that match.
Consumption reverses schema flow while preserving the required
operation set. Every distinct provided operation identified by any required key
or alias receives a correspondence. Alias order has no meaning. A key and alias
identifying the same operation produce one match.

`operations()` retains correspondences in sorted required-key and provided-key
order, with realization before consumption for `Both`. Each retains `role()`,
`required()`, `provided()`, output then input results, including failures. A missing
operation remains exactly one correspondence regardless of the included role;
its `role()` is `None`. Present matches return `Some(Realized)` or `Some(Consumed)`,
never `Both`. A slot with either schema absent is skipped.
`issues()` reports missing, incompatible, indeterminate and failed checks in that
order of correspondences, output before input. `correspondence()` identifies the
matched provided operation and role for each issue.

Summary policy belongs to the caller. `Verdict::collapse` accepts verdicts only,
using `indeterminate > incompatible > compatible`. The report and correspondence
have no collapse method that interprets comparison failures.

```rust
use openbindings::Document;
use openbindings_schema_comparison::{check_interface, ComparisonContext, ComparisonLimits, OperationRole, Verdict};
let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"echo":{"input":true}}}"#)?
    .validate()?.conformant().unwrap();
let context = ComparisonContext::new(document.document().value().clone());
let report = check_interface(&document, &document, &context, &context,
    &|_, _| Some(OperationRole::Both), &ComparisonLimits::default())?;
// This caller counts comparison failures as indeterminate.
let outcomes = report.operations().iter().flat_map(|operation| {
    let missing = operation.is_missing().then_some(Verdict::Incompatible);
    missing.into_iter().chain([operation.output(), operation.input()].into_iter()
        .flatten().map(|slot| slot.as_ref().map_or(Verdict::Indeterminate, |c| c.verdict())))
});
assert_eq!(Verdict::collapse(outcomes), Verdict::Compatible);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`check_interface_with_control` adds cooperative cancellation. One allowance covers
the entire fold, including shared document contexts. Setup failure returns no
report; after setup, a failed slot stays in the report and later checks continue.
Exhaustion is sticky and remains in slots. Observed cancellation always returns
an error with `kind() == ComparisonFailureKind::Cancelled`, including during the final slot. Normalized
reference targets are reused within the call when scope and depth agree;
reference-rule calls retain their path-dependent accounting.
Release measurements of self-comparison with 40 shared eight-property records
at 1,000 and 3,000 operations complete under default limits. See the dated
[limits measurements](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md#limits-principle-november-2026)
for peak memory and elapsed time. Work units are not a portable time estimate.

## Source puller's reference rule

`identical_with_references(left, right, accounted_for, limits)` compares optional
slots using the source-puller rule. The callback receives a `ReferenceContext`
with decoded spelling, the original schema, its resource root and base, plus
`target()` for the initial resolved target or its resolution failure, and
decides accounting; the companion never reads a pull report.
References at corresponding schema positions spelled the same on both sides
and accepted by both callbacks remain leaves. An accounted spelling at one
property cannot keep an inline counterpart at another property as a leaf.
Union and conjunction branches can match spellings across their variants.
The rule also applies to references under retained applicators and `$dynamicRef`. Their siblings normalize, but stay unmerged, as does an allOf containing
such a leaf. Other references are followed. All other identity normalization,
exact equality and failure rules apply. `identical_with_references_and_control`
adds cooperative cancellation; each call uses one shared work and memory policy, including
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

```rust
use openbindings::JsonValue;
use openbindings_schema_comparison::{ComparisonContext, ComparisonLimits, identical};
let context = ComparisonContext::new(JsonValue::parse(
    r#"{"schemas":{"A":{"type":"string"},"B":{"type":["string"]}}}"#)?);
let a = context.schema(context.document().at("/schemas/A").unwrap().to_owned());
let b = context.schema(context.document().at("/schemas/B").unwrap().to_owned());
assert!(identical(Some(&a), Some(&b), &ComparisonLimits::default())?);
# Ok::<(), Box<dyn std::error::Error>>(())
```
