# Original-default large-value capacity review

Bounded PASS for the unknown-payload fixed-meta reservation correction and scalar literal equality. Baseline: `db6b9d2`. Exact production identities, dependencies, tests, driver commands and observations are retained beside this receipt. No production source was edited by the independent reviewer. This is not closure of the default-engine retirement, all-host performance, or a universal allocation guarantee.

## Independent decision

The preserved job exposed two unnecessary representations rather than a need to raise limits. Its two reached supplied resources each carry a 17 MiB `x-pad` string. The fixed standard meta validators do not read either payload, but the old local reservation multiplied each complete string extent by 64. The later contained `const` has a 33,554,434-byte string token. `Values::intern` retained a UTF-16 copy solely to obtain a scalar identity, exceeding the default 48 MiB proof-storage allowance before exact comparison. The original inputs and defaults remain required positives.

The accepted minimal solution keeps the fixed-meta operation and ordinary interpreter semantics intact:

1. Cut only values whose member names are absent from the complete selected fixed meta graph's constrained-name set. Keep names, object punctuation and a one-byte placeholder for every omitted value. Preserve known-keyword operands, malformed kinds and all existing child/reference cuts.
2. For `const`/`enum`, compare unlike JSON kinds directly; compare null/Boolean directly, strings by exact UTF-16 iteration, and numbers by the existing canonical exact `Decimal`. Retain canonical interning for same-kind arrays/objects, `uniqueItems`, and effects that require identity.

Type-only cuts of additional known operands were deliberately not added. They can be separately justified if a real job requires them. A general equality helper that silently returns false on work exhaustion would be unsuitable. This implementation propagates Graph failures instead.

## Fixed-meta proof and source review

`fixed-meta-keywords.json` is the independently enumerated union of root `properties` from every same-instance resource reached by the selected meta entry. It has 46 Draft-07 names, 59 Draft-2019-09 names and 61 Draft-2020-12 names. These are syntax constraints, not the evaluator's active vocabulary selector list. In particular, deprecated `definitions`/`dependencies`, modern `$recursive*` constraints and edition-specific names are preserved.

The complete localized graph has no root-wide equality, `propertyNames`, `patternProperties` or unknown-property validator that could inspect these omitted values. The existing selected-entry plus entire-resource snapshot must remain a required guard; a partial recursive-edge audit would not suffice. The new constrained-set assertion is supplementary, not a substitute for that snapshot/options guard.

The locked jsonschema 0.58.6 `properties` `is_valid` path only enters child validation after a declared name matches. FlatJson member iteration returns a node view and does not traverse the associated value. Fixed string presentation is injective relative to ASCII meta keywords: a name with a lone UTF-16 unit cannot become one of the constrained ASCII names. Its original name extent remains charged, including fixed presentation allocation. Escaped spellings of known ASCII names still resolve to the known name and retain their operands.

The production patch exactly matches those sets. Checked subtraction remains disjoint and overflow-sensitive. Cold fixed-compiler reservation, the 64× local factor, `+4096`, shared Attempt, post-check and actual custom-meta gate are unchanged. No source value or meta-schema is rewritten.

## Literal equality proof and source review

A kind mismatch is a complete inequality proof and requires no scalar parse or composite identity. String equality compares every UTF-16 unit, including lone units, paired units, escaped spellings and literal private-use-marker text. The implementation precharges the source-dependent work and checks cancellation between chunks of at most 1024 unit pairs. It allocates no decoded string.

`Decimal` has a canonical exact representation and derived equality. Its equality is not the magnitude-order routine that can allocate extra exponent-comparison scratch. Each operand still passes through `number()`: its parse work remains charged, the proven i64 branch remains inline, and the non-i64 parse allowance remains. No floating-point or lexical-only numeric comparison is introduced.

Composite `actual_identity` is shared only within one primitive invocation and always denotes the same subject. Scalar comparisons do not populate it. Composite enum choices reuse that subject identity, while each original choice is interned normally. `uniqueItems`, effect identity and other original-arena identity users are untouched. The enum loop continues to visit every choice; this change does not add a general short-circuit promise.

## Independent execution evidence

The same actual-library probe passes on Rust 1.99.0 and minimum Rust 1.88.0, with 34 observation rows on each. After excluding absolute live-baseline values, results and requested-allocation observations are identical.

- 461 actual bounded fixed-meta checks cover all constrained names and escaped ASCII variants, every JSON kind under unknown names, lone-unit/marker-like names, large names, wrong known kinds, dependencies/required arrays, cold/warm behavior and exact/one-below sticky allocation limits.
- 100 const/enum observations cover 25 independently specified equality pairs in both detail modes: exact decimal aliases, negative zero, huge positive/negative exponents, beyond-binary64 differences, escaped strings, astral/lone units and composite fallbacks.
- 18 allocator lanes compare strings at 1 KiB, 1 MiB and 32 MiB plus quotes, including equality, last-unit inequality and length inequality, in Outcome and Explain. The large equal call requests 24 bytes; unequal Outcome requests 73 bytes; unequal Explain requests 410 bytes with 370-byte peak. Each returns to its pre-call live baseline.
- 12 warmed fixed-meta allocator lanes compare 1 KiB and 17 MiB opaque payloads in all editions with ordinary and lone-unit names. Ordinary `x-pad` requests zero bytes; lone-unit name presentation requests 8/56/64 cumulative bytes by edition, with 8-byte peak. Neither depends on payload size. Local reservations are respectively 4800/4864 bytes.
- All 168 observed long-equality cancellation checkpoints across both detail modes refuse as Cancelled and permit clean retry. Exact evaluation work 2,097,305 succeeds; one below returns `evaluation-work-limit`.
- An actual custom meta constraining `x-pad` still accepts the matching value and rejects the mismatching value at preparation.
- The complete preserved large-document/two-resource/large-const job passes independently on both toolchains at original defaults. Its test was renamed by the parent; its values were not weakened.

The initial driver put a lone surrogate inside the OBI document, which correctly failed OBI-01. That driver error and log remain. The corrected equality driver places the same schema bytes in a supplied resource, as the established exact-string controls do; no expected equality was changed.

## Boundaries

This reservation is an SDK cooperative accounting policy, not a claim about every dependency operation or allocator request. The local cut does not apply to arbitrary selected custom metas. Large known opaque operands in supplied standard schemas still retain their current conservative fixed-meta reservation. Composite interning remains and can still legitimately encounter its configured allowance. Whole Wasm/browser qualification belongs to the parent's separately recorded current-host gate.
