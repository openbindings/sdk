# Prepared numeric operands: independent design review

**Decision: reject eager-all construction and removal of schema-dependent runtime charges. Proceed with a bounded optional cache plus original-operand fallback.** This is a design decision, not an implementation PASS. No production file was edited. The two findings below are acknowledged by the implementer.

The source basis is SDK `0b9dc0466d77623f2ed61b43c02a00ab98e75710`, specification `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`, and interfaces `5815fb106bddd2c85ff4e93e0077570b3f6b9e74`. Exact reviewed file hashes are in [source-hashes.json](source-hashes.json). Public-documentation and unrelated TypeScript option cleanup underway during review are outside this gate.

## Authority and boundary

Pinned OpenBindings §5 establishes exact decimal equality; §5.5 delegates keyword meaning to JSON Schema and distinguishes undefined results, unavailable content, and decisive branches. OBI-10 continues to govern contained schema admission. Caching a reading changes neither that admission nor the selected original resource/profile.

The five predicates are mathematical comparisons or divisibility checks; the length/cardinality bounds accept nonnegative mathematical integers. Counts above the largest representable instance cardinality can saturate at `u64::MAX` without changing comparisons on supported hosts. Fractional/exponent spellings such as `1.00`, `1e0`, and `100e-2` remain integers. `minContains`/`maxContains` do not apply without `contains`; inactive vocabulary fields are not operands. See [Validation §§6.2–6.5](https://json-schema.org/draft/2020-12/json-schema-validation#section-6) and [Core §7.6.1](https://json-schema.org/draft/2020-12/json-schema-core#section-7.6.1).

Invalid shapes allowed through a deliberately permissive custom meta-schema are local capability readings in the current evaluator. This review does not turn those into new normative Undefined results, nor use their characterization to weaken standard meta-schema admission.

## R-N1 — P1: eager-all parsing changes a funded, unused operand into preparation refusal

The current original Plan prepares and returns **S** at defaults for:

```json
{"properties":{"unused":{"minimum":111...111}}}
```

with value `{}` and 100,000, 1,000,000, 2,500,000, or 3,000,000 digit tokens. The child never evaluates. Current per-operand accounting in `semantic/primitive.rs:83–88` is `8L+1` work and `16L+256` allocation. Moving that read unconditionally into preparation consumes 20,000,001 work at 2.5 million digits and 24,000,001 at 3 million, exceeding the default 20,000,000 even before other preparation work.

This is an admission-timing/default-capability regression, not a claim that the specification fixes the SDK's numeric budgets. It is material to the requested unchanged default jobs and repeats the eager-VM construction hazard. These inputs are within current JSON ingress limits and require no decimal exponent expansion.

The same result holds for a 3-million-digit `maxLength` beneath the missing property. A 3-million-digit orphan `minContains` also returns S and performs no evaluation allocation. Exact generators, source hashes, original observations, and probe identity are retained in [unused-eager-hazard.json](unused-eager-hazard.json) and [unused-count-observations.json](unused-count-observations.json).

**Required correction:** select oversized/excess operands for original fallback from their existing token length before parsing, copying, or making the parsing reservation. A per-token ceiling alone is insufficient: bound cumulative eager spelling and record ownership too. Do not try a reservation, catch its failure, and continue with fallback; preparation exhaustion is intentionally sticky.

## R-N2 — P1: a retained Bound still allocates from the schema operand during comparison

`numeric::Bound::admits` does more than parse the instance. `Decimal::compare` routes wide comparisons through `compare_inline_wide`/`Wide::compare`, which call `Power::offset`. For a large written exponent, `offset` clones and adjusts its digit text. Retaining the parsed Bound does not remove that work or allocation. Relevant sites are `numeric.rs:70–92`, `decimal.rs:40–96`, `565–591`, and `678–693`.

The independent counting-allocator probe parses `minimum: 1e<digits of 9>` **before** measurement and measures only `bound.admits("1")`:

| Exponent digits | Requested allocation | Incremental live peak | Result |
| ---: | ---: | ---: | :--- |
| 100 | 303 B | 202 B | F |
| 1,000 | 3,003 B | 2,002 B | F |
| 100,000 | 300,003 B | 200,002 B | F |
| 1,000,000 | 3,000,003 B | 2,000,002 B | F |

There are four allocation operations in each measured call and no retained growth. `size_of::<Bound>()` is 40 on the tested toolchain, which plainly does not bound this scratch. [cached-comparison.rs](cached-comparison.rs), its [build identity](cached-comparison-build.json), [original results](cached-comparison-results.jsonl), and [independent repeat](cached-comparison-rerun.jsonl) preserve the witness. These measurements demonstrate the omitted cost; they are not a general upper-bound proof.

**Required correction:** retain source length and the existing conservative schema-dependent work/allocation reservation for all five Bound predicates, not only `multipleOf`. Borrow the Bound; a per-call `clone()` can itself copy the wide decimal. Keep instance parsing and `multipleOf`'s separate scratch/work/domain limit. Removing comparison reservations requires a separately audited allocation-free comparison or prepared magnitude representation.

## Minimal implementation sequence

1. **Group by original node.** Add one immutable optional record per admitted original node, with entries only for present, active numeric/count keywords. Each entry is a parsed reading or an original fallback. A missing cache entry means “read the original operand,” never “the keyword is absent.” The Plan already owns the source arenas. Do not duplicate JSON, URI keys, or controls. Obtain a record once per body and iterate its actual predicates; contains counts still execute at their own array-applicator site.
2. **Make preparation optional and bounded.** Prioritize count copies, then small numeric Bound readings. A concrete conservative starting policy is a 64-byte token ceiling, 64 KiB cumulative eager token bytes, and 1,024 eager operands, including invalid-reading records. These are proposed private cache-policy ceilings, not normative limits or proven optimal values. Bound node-record/map ownership separately and reserve from actual layout plus the existing audited container schedule. Once any ceiling would be crossed, leave subsequent entries on original fallback. This admits ordinary decimal/exponent spellings without allowing one unused large token or many small tokens to create unbounded eager construction.
3. **Choose before charges.** Check original kind/length and the cache-policy counters before source parsing/reservations. For selected readings, charge parsing and record ownership before construction under the existing shared Attempt. Wrong-kind payloads need only a small local marker; do not traverse or clone them. Integer count conversion can keep the exact existing Decimal implementation; a separate fast parser is unnecessary for this increment.
4. **Preserve demand and errors.** Activate through the original `Profile`/`Interpreted::get` and the already reviewed closure. Do not descend opaque data, orphan branches, or ignored draft-07 reference siblings. Do not prepare orphan contains counts as assertions. At runtime, surface invalid numeric/count readings only where the corresponding old read happened. In particular, primitive invalid shapes currently become local inability effects, whereas malformed contains counts return a typed refusal from the active array path. This representation change must not silently move either into blanket preparation failure or invent new branch masking.
5. **Keep honest runtime costs.** Parsed counts remove their schema parsing/conversion completely; numeric readings do not yet remove schema-dependent comparison cost. Raw fallback uses the existing charged parse and arithmetic path, only when demanded. Retain exact instance parsing, finite multiplication/division limits, and original predicate/instance/effective-context effect identity. If per-call fallback memoization is later added, charge it and keep it local to the Run; it is not necessary for the first slice.
6. **Publish only completion.** No preparation control or partially filled attempt survives in a ready Plan. Cancellation and real allocation/work failure remain terminal and sticky; a later clean attempt may succeed. Immutable records can be shared across concurrent calls, but runtime arithmetic scratch and fallback caches cannot be shared mutably.

### Near-limit qualification

The proposed static cache ceilings prevent the demonstrated disproportionate eager work. They do **not** guarantee every former exact preparation cutoff remains identical: selected optional work is still real work. Current `Control` exposes charging, not a non-consuming remaining-capacity query. A “try then defer” implementation would violate the sticky contract. If preserving preparation success at every previously sufficient explicit cap is required, optional selection needs a checked, non-consuming capacity policy covering both nested work and allocation, or must stay deferred. Do not add a general budget subsystem merely to imply that guarantee. For this bounded increment, retain low-limit refusal/retry controls and explicitly characterize any changed cutoff; defaults and existing required jobs must remain unchanged.

For the example cache policy, eager source reservations alone are bounded by `16 × 64 KiB + 256 × 1,024`; work by `8 × 64 KiB + 1,024`, plus measured/charged lookup and record construction. This does not certify map allocation, deferred runtime arithmetic, or complete preparation; those remain charged independently. A smaller initial policy is acceptable. No default resource limit should increase.

## Independent controls and gate

[design-controls.json](design-controls.json) contains **59** exact-token cases plus four generated stress families and lifecycle/resource controls. Expected results were assigned from keyword meaning and the explicit existing capability boundary, before replay. [baseline-summary.json](baseline-summary.json) records all 59 passing through the frozen original-Plan bridge in both Outcome and Explain, including owner disposal and initially cancelled calls. This is pre-change characterization, not proof of an unimplemented optimization.

The control set covers all five comparisons/divisibility forms, every count family, exact large decimals/exponents and negative zero, Unicode scalar length versus lone units, counts beyond `u64`, orphan/inactive fields, draft-07 activation, permissive custom invalid operands, original-node reuse, missing content, and decisive branches. The generator corrected an extra closing brace in its initial orphan-then input before final capture. Its inactive-validation case uses `[1]`; a separate empty-array F control preserves the still-active `contains` default. No SDK behavior or semantic oracle was changed to obtain the final result.

Implementation acceptance additionally requires:

- Both R-N1 stress families remain Ready and S at unchanged defaults; fallback is selected before source-sized work/allocation and no partial cache survives failure.
- The R-N2 comparison witness is still covered by runtime reservations; funded positive and low-budget refusal lanes, including wide/wide and opposite-sign comparisons, are retained. Counts demonstrate actual removed parsing cost without a general multiplier reduction.
- Cache policy exact/one-over boundaries, aggregate many-small-token overflow, malformed custom operands, every observed cancellation boundary, retry, concurrent use, source-owner disposal, and replacement-snapshot isolation pass.
- Forced original-Plan replay of all 1,546 required value cases, existing exact-primitive/custom/recursive suites, and unchanged 20,000-item/late-invalid/50,000-property jobs pass. Default-backend success alone is insufficient evidence for this increment.

**Gate:** design may proceed with the bounded optional-cache constraints above. Eager-all, erased comparison reservations, implementation correctness, full once-prepared numeric coverage, and broader original-engine/performance completion are not approved by this receipt.
