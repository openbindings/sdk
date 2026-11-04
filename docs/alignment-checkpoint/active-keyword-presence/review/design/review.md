# Mask-only original-node dispatch: bounded design approval

Approve the mask-only variant for implementation review. Replace the leaf set with one compact descriptor map, retain existing predicates/application algorithms and original source locations, and use complete absence facts to avoid raw lookups and empty phases. Do not retain member slots, parsed type records, another child graph or empty numeric records in this increment. No SDK edits or executable probes were performed for this design review.

The earlier constant-wrapper semantic PASS remains valid but is not a performance endorsement: parent measurements found the wrapper-only candidate slower on scalar and reference jobs, so that candidate is discarded. The borrowing-only change is separately retained as a small ownership simplification. Those measurement histories remain in their original receipt directories. This proposal does not predict baseline recovery or accept any G6 residual.

## Source premise and certificate boundary

The inspected `Profile::get` has exactly two filtering conditions: inactive edition/vocabulary, or Draft-07 reference-object sibling suppression. Otherwise it returns `schema.get(keyword)`. It does not synthesize operands, infer a default, translate one keyword into another or consult dynamic scope. Therefore a **complete original member scan**, with each mapped field filtered by the original profile, can certify absence for a closed keyword inventory.

The reference-object condition uses authored `$ref` presence, even when its operand is malformed; do not replace it with successful query/target detection. A known vocabulary marked optional (`false`) remains active; an omitted custom vocabulary is inactive. Unknown required vocabularies and unestablished profiles remain opaque. Escaped keyword spellings are decoded by the existing exact name accessor; an unknown non-scalar name cannot be a recognized ASCII keyword.

Only activated original nodes with the existing eligible-resource, good query-inventory and known-profile checks may publish a known mask. Unknown/ineligible nodes need no map entry: the default lookup is explicitly `Original`. Preserve current Boolean-before-profile behavior in `begin`; absence of a certificate is not permission to turn a previously handled Boolean into a blanket profile refusal. Boolean schemas with an established profile can have the ordinary complete empty-mask/leaf certificate.

## Minimal representation and construction

Use a small Copy wrapper around one `u64`. Reserve distinct bits for `Original` and the existing certified-leaf fact. `Known(0)` is a complete absence certificate; a missing descriptor defaults to `Original`, never zero. An Original descriptor must not be interpreted as a leaf. No particular bit numbering is prescribed, but assert that the closed keyword set and flags fit without overlap.

The current runtime inventory fits in 43 bits:

```text
type const enum minimum maximum exclusiveMinimum exclusiveMaximum multipleOf
minLength maxLength minItems maxItems minProperties maxProperties uniqueItems
required dependentRequired dependencies format pattern
$ref $dynamicRef $recursiveRef
allOf anyOf oneOf not if then else
dependentSchemas properties patternProperties additionalProperties propertyNames
unevaluatedProperties
items prefixItems additionalItems contains minContains maxContains unevaluatedItems
```

Construct a provisional mask in the **existing full field loop**. Decode the field name once using its existing charged read. For a mapped keyword, use the unfiltered-by-mask original `Profile::get`/`Interpreted` reading to decide presence, then set its bit before malformed-child or orphan filtering. There is no new pass of 43 unconditional lookups per node. For unmapped fields, preserve the old child/certificate traversal rules without claiming a keyword bit.

Crucially, the mask under construction cannot filter preparation reads: while scanning a leading `then`, a later `if` has not yet been visited; likewise `additionalItems` may precede `items`. Keep a separate provisional accumulator and an `Original` preparation view until the scan completes. Preserve the existing query, pattern, format, numeric, annotation-demand and child/custom admission passes and their shared construction allowance. Post-check and reserve before publishing the finished map entry. An interrupted scan cannot publish a partially empty mask.

Keep the old leaf criterion independently: active references disqualify it; active child-shape keywords and dependencies/additionalItems disqualify it by **presence before shape/applicability filtering**. Do not define leaf as “no phase bits.” `$defs`, definitions and contentSchema, although outside the 43 runtime bits, currently conservatively disqualify leaf and must keep doing so. Empty/malformed anyOf/oneOf and orphan branches also retain the established certificate rules. Refinement's foreign/custom leaf no-producer shortcut must consume this same leaf fact.

## Runtime use and phase groups

`Interpreted::get` can return None early only when a known descriptor lacks the bit for that **mapped** keyword. For present fields it still calls the original `Profile::get`; for an unmapped keyword or Original descriptor it always calls `Profile::get`. This makes optional numeric-cache misses continue through their existing Original fallback and avoids accidentally filtering preparation-only names such as `$recursiveAnchor`.

Load/copy the descriptor when entering the actual selected node. Refresh it after alias normalization changes the node; never carry the starting alias's mask onto its target. Put that copy in the existing Frame and pass it through `Frame::schema` to avoid a map lookup per keyword/phase. The descriptor does not replace Visit's original node/instance/scope identity, original child-position lookup, effect identity or diagnostic seed. Reusing it across different dynamic contexts is sound only because it describes the same original node/profile; dynamic target selection still happens normally.

For this first variant, skip whole phases only when the known mask has none of the following fields:

| Phase | Conservative trigger inventory |
| --- | --- |
| Logical | allOf, anyOf, oneOf, not, if, then, else |
| Object | dependentSchemas, dependencies, properties, patternProperties, additionalProperties, propertyNames, unevaluatedProperties |
| Array | items, prefixItems, additionalItems, contains, minContains, maxContains, unevaluatedItems |

Keeping orphan then/else and min/maxContains as conservative triggers avoids another applicability optimization. Original descriptors always enter the existing phases. A skipped phase leaves State untouched, including annotations and witnesses produced by references/logical siblings. Do not reset State, manufacture false coordinates, or discard a returned child before its producing phase consumes it. Existing absent object/array phases add no annotations; global annotation demand alone does not require their name/item scans.

All present active keywords keep their current behavior, even when malformed, empty, nonmatching, in an unchosen conditional arm, or alongside a decisive sibling. Do not skip a present Limited predicate or a currently eager child. Do not initially combine this with per-member phase shortcuts, cached child targets, alias flattening, numeric parsing changes, eligibility-check removal, context redesign or Graph callback batching. Phase-trigger inventories must be reviewed alongside any future new/transitive keyword reads; the mapped-keyword fallback alone does not automatically make a newly added phase algorithm safe to skip.

## Footprint, work and capacity

The payload becomes one original-node key plus one u64 per certified activated node, instead of a set entry only for certified leaves. There is no per-member or operand payload and no copied JSON/name. Ineligible/unknown nodes can remain absent from the map. Do not allocate a dense table for unused environment nodes. The extra descriptors for non-leaves are real retained growth even though the separate leaf set is removed.

Use the existing explicit SDK map-reservation convention, adjusted/documented for the actual key/value payload, and checked arithmetic before insertion/growth. This is a scoped SDK reservation estimate, not a new universal HashMap allocator certificate. Charge the bounded mask construction/publication work at the existing field/insertion sites; do not add a 43-probe preparation preflight. Frame backing already reserves actual `size_of::<Frame>()`; retain that mechanism and record actual native/Wasm layout after adding the mask. Do not assume the field fits existing padding.

Keep all preparation and evaluation defaults unchanged. Existing numeric token/aggregate/record cache limits remain optimization limits with Original fallback. Actual shared Attempt exhaustion stays sticky; no exhausted allocation is caught to invent an empty mask. Do not introduce an optimization cap in this patch unless needed by evidence; if introduced later, select it before charging and fall back to Original while preserving any independently required leaf certificate.

Removing absent lookup/phase work can change minimum successful work and callback totals. This is different from the prior count-preserving Graph patch. Retain schema entry, scope, actual child, loop-progress, and completion checks; recompute exhaustive observed interruption boundaries on the new work path and preserve old receipts. Do not promise globally identical callback counts between separately allocated owners, since the existing address-keyed pattern lookup already charges actual collisions.

## Qualification boundary

`controls.json` contains 20 revised groups, including an exhaustive bit/get equivalence check, provisional-mask ordering negatives, every phase trigger, aliases, unknown profiles, malformed/empty applicators, numeric-cache fallback, annotation/refinement and exact origins. Public default-capacity jobs include the original 20k/20001 arrays, 50k object, 700k numeric array, 1000/5000 references, large opaque/const positives and pattern workloads. Add direct default preparation metadata measurements so a faster evaluation does not conceal preparation bloat.

Run the required 1546 and auxiliary/reference obligations without semantic oracle changes; retain scoped exact/one-below limits, sticky first cause and every observed cancellation checkpoint. Then measure the unchanged selected requests under the matched protocol, reporting residual timing and memory differences. Approval here covers this bounded design, not its unreviewed implementation or eventual performance outcome.


## Implementation-review correction

The later implementation review corrects one source premise above: `$defs`, `definitions` and `contentSchema` did **not** exclude a leaf under the original `child_shape` predicate. They remain inventory/syntax positions, but are not value-applicator children. The original text and initial failed control are preserved; see `../implementation/review.md` and its before-leaf-correction evidence. This correction does not alter the independent-leaf requirement or authorize deriving leaf from absent phase bits.
