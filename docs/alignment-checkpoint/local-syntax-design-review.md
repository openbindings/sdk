# Local syntax admission: independent design review

**PASS for the narrowed design, not for an implementation.** A first increment limited to a proved per-position footprint and a source-tied meta-edge guard is coherent. Keep cross-phase syntax-fact sharing separate. No public contract, admission verdict, selected meta-schema, default limit, or preparation error boundary needs to change. No production source was edited.

The reviewed SDK source is `6988e6d252ea224a77b464c9ed83e6d28be3f22f`; the design and implementation dependencies are pinned in `source-hashes.json`. This review does not close the ordinary compiled-backend allocation certificate or claim whole-preparation timing/stack qualification.

## Normative and implementation boundary

Pinned OpenBindings §5.5 and OBI-10 require admission of every contained schema and its structural subschemas, including unused definitions. The external standard-profile admission policy and actual custom-meta checks likewise remain in force. Replacing recursive meta edges with local object/Boolean checks is already the fixed validator's behavior; the proposed change accounts for that behavior and does not introduce lazy admission.

`fixed_schema::valid` calls the backend's `is_valid` path. Its localized recursive edges check only `type: [object, boolean]`; they neither run a child schema nor collect child diagnostics. `JsonRef::text`, `kind`, `members`, and `elements` use the existing arena's spans/indexes. Enumerating immediate children and measuring their exact token extents need not traverse their descendants or allocate. The candidate footprint therefore can exclude those descendants while preserving the actual validator call on the complete original value.

`meta_syntax` must continue traversing all declared schema positions; `check_from` has different evaluation reachability and cannot substitute for it. The helper applies only to the fixed standard syntax validators. **Never apply its cuts to evaluation against an actual custom meta-schema:** such a schema can inspect arbitrarily deep fields, including fields named `properties`, `$defs`, or `default`. A custom profile's meta carrier still receives its own required standard/custom admission, with its original identity and dialect.

## Concrete read-shape audit

`audit-meta.py` independently inventories the **16** fixed standard resources: one draft-07, seven 2019-09, eight pinned OBI 2020-12. All **51** recursive edges currently replaced by the localization functions are single-member reference objects; each replacement is precisely the object/Boolean type check. `meta-edge-audit.json` records every exact original pointer and file hash. Nonrecursive references to local definitions, schema-array constraints, string-array constraints, root constraints, and all scalar checks remain active.

The allowed first-stage cuts are:

| Shape | All editions | Edition-specific additions |
| --- | --- | --- |
| Object-map child values | `definitions`, `properties`, `patternProperties` | `$defs`, `dependentSchemas` in 2019/2020 |
| Array child values | `allOf`, `anyOf`, `oneOf` | tuple `items` in draft-07/2019; `prefixItems` in 2020 |
| Direct child | `additionalProperties`, `propertyNames`, non-tuple `items`, `contains`, `not`, `if`, `then`, `else` | `additionalItems` in draft-07/2019; `unevaluatedProperties`, `unevaluatedItems`, `contentSchema` in 2019/2020 |
| Mixed map | `dependencies` in **all three editions** | Cut only object/Boolean children; retain string-array operands in full |

**Resolved design clarification:** the deprecated `dependencies` checks are present in both the 2019 and 2020 dialect roots. They are not merely a draft-07 concern. A duplicate string array is invalid in all three; its uniqueness/equality work must remain charged. This clarification is now in the design.

For the first increment, cut only object/Boolean child tokens. A wrong-kind containing map/array, invalid direct operand, or array-valued dependency keeps its original extent. This is deliberately conservative. Keep full extents for `const`, `enum`, `default`, `examples`, all scalar operands and unknown fields. In particular, the pinned draft-07 `enum` check requires uniqueness/nonemptiness; the later two copies do not. Retaining all enum bytes avoids conflating those different local read shapes.

## Required accounting invariant

For an original position with raw extent `R`, let `C` be the disjoint immediate child token ranges established by the guarded table. Compute:

`L = R - sum(child_token_bytes for C) + kind_unit * |C|`

Use a positive kind unit, keep every parent/member spelling, separator and container delimiter, and never select nested ranges inside an already selected child. An allocation-free fold over borrowed immediate headers is enough. The independent probe uses one unit; a larger justified constant is also safe. Do not use serialized/reconstructed JSON to compute `L`.

The footprint scan itself must be cooperatively charged **before** consuming each member name/child header, including headers that will later be subtracted. Retain the existing cold fixed-compiler allowance and the existing `64*L + 4096` allocation / `64*L + 1` work schedule for the fixed check, then the final cancellation/sticky-failure check. Arithmetic must refuse on overflow/underflow rather than wrap or silently reset a refusal. Do not infer a smaller multiplier from these observations.

The root property's name, a schema-map entry's name, and escaped/non-scalar UTF-16 spelling remain in `L`; fixed presentation may allocate while reading such a name even though the child payload is skipped. The source-sized numeric/URI/string operands remain in `L`, including exact large exponents. Wrong-kind operands remain conservatively charged even if the validator returns early. Draft-07 reference-object suppression and foreign resource inheritance remain exactly as before; a separate shortcut for that case is unnecessary for this increment.

## Source-tied guard

The selector table alone is not authority. A guard must cover the complete source-owned fixed-meta graph and localization/options, not just count recursive markers. Adding an ancestor `uniqueItems`, `const`, child-inspecting sibling, or constraint on a map could make the old subtraction unsound even if all 51 reference markers remain unchanged. Pin/check the full fixed resources and assert the allowed localized shapes/paths, including constraints on their containing maps/arrays and the no-format-assertion policy. A change to localization, backend entry mode, fixed-meta graph, or schema-position inventory requires re-review or conservative fallback to the full raw extent.

**Concrete guard recommendation:** compare the complete normalized localized-resource graph, keyed by exact resource URI, against a checked-in, independently reviewed canonical JSON snapshot. Assert the exact 1/7/8 resource inventories. Produce the actual snapshot by calling the same private localization functions used by compilation (the existing nested foreign helper can be extracted privately); do not independently reimplement localization or regenerate the expected snapshot during the assertion. Keep `verify-spec`'s exact publication-byte checks as a separate prerequisite. This requires no new Rust hashing dependency and no production runtime registry. The complete snapshot is preferable to a partial edge-only certificate: an added ancestor or opaque equality constraint changes it even when every recursive marker is unchanged. Separately preserve/test the read-relevant entry/options contract: `is_valid`, formats disabled, exact numeric overrides, and fixed string presentation. Snapshot updates require reviewed disposition, not automatic acceptance of source drift.

Tests should mutate copies of the guard inputs: add a child-inspecting sibling beside a recursive marker, add a constraint at the containing map/array, change a localized reference spelling, or change the schema-array/string-array definition. Each must reject the certificate, rather than merely adjust a digest without semantic review. This guard need not compile a second production validator or introduce a new schema registry.

## Independent evidence

`run.py` compiles a small private probe against the current SDK artifacts. The probe leaves the fixed validators untouched and computes a separate candidate extent. **210/210 controls pass** their independently assigned local-validity and cut-count expectations. The extent helper allocates zero bytes in all rows; every measured warm fixed check fits the retained candidate allowance. The controls cover every table family in every edition, invalid containing shapes, dependencies equality, exact surrogate strings/names, escaped keyword names, exact huge numbers, enum differences, ignored draft-07 reference siblings, width 1/128/2048, and depth 1/16/64.

Representative observations:

- `not` with a child growing from 32 to 100,030 raw bytes has the same candidate extent 9 and the same 16 charged backend reads; the warm local validator allocates zero bytes in both.
- A 64-level `allOf` tree has a root candidate extent 13 and 18 charged reads; each descendant still needs its own separate admission check.
- A 2,048-property map preserves 17,338 local bytes, 2,049 header visits, and 2,065 charged backend reads; it is not incorrectly treated as constant work.
- A 100,031-byte string-array dependency keeps all 100,031 bytes, with no cut.
- The exact non-scalar property-name row allocates 8 bytes in fixed presentation; its name bytes remain in the 27-byte candidate extent.
- Draft-07 rejects two equal 100k-containing enum values while 2019/2020 accept them, matching their actual fixed schemas. All retain the full 200,056-byte operand extent.

These are supporting structural/allocation observations, **not a finite-proof coefficient or a production resource-gate pass**. Backend charged-read counts do not include every scalar byte scan. Parsing/input ownership is outside the measured warm fixed-check window. Cold fixed compilation, full syntax traversal, actual custom-meta validation, and candidate production cancellation/limits remain implementation gates. The private probe has no cancellation adapter because it is measuring the existing original validator and the candidate mathematical extent, not claiming the future bounded helper is complete.

## Implementation acceptance gates

1. Port the guarded cut table independently of evaluation `child_shape` filtering. Keep orphan `then`/`else`, `contentSchema`, unused definitions, deprecated `definitions` and schema-form `dependencies` in full syntax traversal. Preserve draft-07 reference-sibling suppression and per-resource profile switches.
2. Compare unchanged validator verdicts and public preparation outcomes. A parent `allOf:[{type:17}]` passes its local type-only check but must fail whole-schema admission; the same malformed structure under opaque `default`/unknown data is not a new schema position. Include those distinctions in all applicable editions and mixed-resource/custom cases.
3. Reject stale certificates for every mutation class above. Preserve every name/header charge, prove disjoint ranges, test empty/Boolean/wrong-kind inputs, maximum supported source extents and arithmetic refusal. Confirm the helper allocates nothing on valid and invalid paths before typed failure construction.
4. Exercise cold and warm standard validators, exact/one-below work/allocation limits, cancellation at every observed new scan/check boundary, clean retry, nested aggregate attempt failures, and no false Ready after ignored failures. No syntax-fact cache is introduced in this increment, so no new cache publication claim is needed.
5. Include large dependencies/required/type/enum arrays, huge numeric operands, long headers, exact unpaired units, wide and deep schema trees, actual custom-meta constraints that inspect nested contents, invalid unused positions, and missing required meta content. Retain current failure classifications.
6. Replay the full 1,546 required suite through direct original Plan, the unchanged 20k/20,001/50k default workloads, current original-engine/recursive/admission controls, and the normal native/host gates. Add a separate allocator lane for the local check and full preparation; neither successful local probes nor reduced reservations establish whole-engine performance.

**Disposition:** proceed with this bounded implementation and return it for review. Cross-phase completion-fact sharing, broader opaque-field reductions, cold fixed-compiler retuning, and the ordinary user-schema compiler remain separate work.
