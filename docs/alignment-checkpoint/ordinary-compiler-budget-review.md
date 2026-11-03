# Ordinary compiled preparation: independent allocation review

**FAIL / open qualification gate.** This report establishes concrete missing reservations and a bounded implementation plan; it does not certify a backend allocation bound. It concerns ordinary user-schema projection/conversion/compilation, separate from the previously qualified original inventory and custom-meta semantic path. Production was not edited.

`PreparationLimits.max_allocation_bytes` is a monotone conservative allowance for temporary, failed and retained preparation work, not resident memory. One `Attempt` already spans construction and custom-meta work. The ordinary compiler currently drops that allocation authority after `compiled_environment`: `prepared`, `defined`, `compile`, `bounds` and `share_names` receive only `WorkControl`. Important allocations before that boundary in `SchemaSpace::program_definitions` also are not charged.

## R-C1 — ordinary preparation completes well above its allocation allowance (P1)

The public API reproduces this with a warmed original environment and a 1,024-byte allowance:

| Original schema shape | Requested live peak | Cumulative requested allocation | Retained Ready artifact |
| --- | ---: | ---: | ---: |
| `true` | 6,175 | 11,386 | 1,491 |
| 1,000 ordinary properties | about 4.48 MB | about 6.96 MB | about 1.97 MB |
| One 100,000-byte property name, also required | about 1.59 MB | about 2.02 MB | about 1.18 MB |
| 1,000 exact enum strings | 578,050 | 656,304 | 321,778 |
| 100 patternProperties | about 1.09 MB | about 3.65 MB | 854,270 |

Every row returns Ready. `baseline.json` contains exact values for all caps and sizes; rounding above is descriptive only. Some first name conversions populate source-owned lazy caches, which are also new allocations during the attempt; their retention is identified separately, not mistaken for a leaked validator.

A companion internal bridge probe localizes an earlier hole: `program_definitions` under `Attempt(max_allocation_bytes=0)` charges **zero**, yet completes an enum projection at 578,002 bytes peak / 612,993 bytes cumulative. A 1,000-property projection also completes at zero charge, with 263,720 bytes peak. Guarding only `options.build` cannot fix this.

Affected work includes reach/projection collections and literal arena creation; definition maps and exact operands; bundle/document cloning; dynamic specialization, cycle unfolding, unevaluated transforms and bounds programs; pattern reading/form storage/build retries; library registry/compiler caches, keyword validators and annotations; retained explanation locations; and failure-path origins. Existing `Limits.program_bytes` counts program *growth*, not all projected storage or the backend's compiled program. Pattern memory limits likewise do not replace the shared attempt.

## R-C2 — diagnostics allocate input-sized origins after refusing (P1)

With `pattern_bytes=1`, a property named with 1k/10k/100k bytes and `pattern:"ab"` returns `Limit(pattern-byte-limit)`, but builds original pointers of 1,040/10,040/100,040 bytes. At a preparation allowance of 1 KiB the requested peaks are 7,904/43,904/403,904 bytes. `lib.rs::keyword_location` calls `space.location` before measuring or reserving it; cycle and some library error paths have similar unbounded materialization calls.

Reuse the already reviewed bounded original-location mechanism: measure pointer size before allocation, cap diagnostic size, reserve before creating/copying it, and retain the original cause if provenance is omitted. For Limit/Cancelled, omitting origin is the simplest safe path. A failed operation must not create a new unbounded allocation phase to describe the failure.

## A linear node/byte multiplier is insufficient

The locked jsonschema 0.58.6 backend eagerly builds unevaluated-property/item structures. Its property-validator cache tracks pending recursive construction and removes the entry after completion (`keywords/unevaluated_properties.rs`, `compile_pending_property_validators`, lines 427–492). Repeated acyclic paths rebuild annotation structures. The regular compiled-node cache does not eliminate this storage.

A linear-size diamond DAG, each level `anyOf:[ref next,ref next]`, with root `unevaluatedProperties:false`, demonstrates this. With an explicit 4 MiB stack allowance, depths 4/6/8/10 all prepare at a 1 KiB allocation allowance; peaks grow approximately 128k / 338k / 1.21M / 4.71M bytes. Retained compiled artifacts grow approximately 78k / 274k / 1.06M / 4.28M. The default smaller stack refuses deeper examples, but increasing that independent supported limit cannot silently disable the allocation bound.

Consequently, no empirical constant times unique schema nodes or source bytes is a sound general backend certificate. Repeated annotation-walk multiplicity, separately initiated holders/builds and retry counts must be included. The probe deliberately stops at depth 10; no unbounded stress run is needed to establish the issue.

## Concrete implementation recommendation

1. Keep the public evaluator seam and `PreparationLimits` unchanged. Pass the **same borrowed internal `Control`/Attempt** through private `prepared`, `defined`, `compile`, `bounds`, pattern-sharing and diagnostics, alongside caller control only where existing signatures need it. Do not start a new allowance at a backend build, missing-resource reconstruction, bounds build, semantic fallback or retry. No attempt/control is retained in `Compiled`.
2. Reserve SDK-owned allocations before each owning boundary. For projection, use existing original inventory/JSON counts to preflight vectors, maps, generated names/addresses and separated literal arenas before creation; for conversion, count JSON container nodes/member names/string bytes, exact operand parsing, name decoding and copied schemas before materialization. Existing `DefinedResource.bytes/nodes` are useful conversion inputs, but are computed too late to cover the projection that creates them. Checked arithmetic overflow must mark the shared allowance exhausted.
3. Account transforms cumulatively. A previously reserved original object is not a reservation for a new clone. Include dynamic contexts, cycle copies, unevaluated helper definitions, bundle-document clones and both bounds/ordinary program builds. Reserve before optional work; never allocate, decide to drop it, then refund or ignore its cost.
4. Give each **opaque backend build** a version-locked structural cost certificate before entry. Count ordinary compiled keyword/cache records, schema/key/string copies, prepared keys and location suffixes, generated registry state and error capacity. For unevaluated walks, count expanded same-instance annotation paths with duplicate edges, separately for every holder and build; a saturating/budget-capped recurrence over the transformed finite graph avoids constructing the expanded structure merely to measure it. Do not assume completed annotation summaries are cached when the dependency only caches active recursion. If a supported sound bound is unavailable for a shape, route to the existing bounded semantic Plan **before** starting that backend build, or return an honest capability refusal. This is not a second evaluator abstraction.
5. Reuse the reviewed pattern preflight machinery before ordinary pattern parsing. Bound engine scratch as well as final compiled storage; reserve every initial build and every `share_names`/bounds rebuild cumulatively. A successful final compiler object does not erase the failed builds that preceded it. Lazy evaluation-time pattern allocation remains governed by evaluation/pattern limits; do not claim preparation limits cover future calls.
6. Preserve failure typing. Exhaustion is `PreparationFailureKind::Limit { resource: "preparation-allocation-bytes" }`, cancellation remains Cancelled, and original resolution failures retain structured causes. A failed optional bounds build cannot swallow a sticky allocation failure and return Ready. Final outer Attempt checks and post-backend cancellation checks must run before returning an owned validator.

This recommendation deliberately supplies an accounting model, not an unaudited numeric multiplier. Constants/capacity rounding need derivation from the locked implementation and supported architectures, then adversarial verification. An allocation observer can falsify an underestimate; passing samples cannot prove a universal bound. A generic global-allocator replacement or post-hoc memory measurement is not required or recommended for the public SDK.

## Controls and gates

`probe/` is a standalone counting-allocator program against the public API plus a narrowly labelled internal projection probe. `baseline.json` retains **86 observations**: 16 public shapes × five allocation caps, three projection probes and three error-origin probes. Inputs/admission/environment warming precede each measured prepare. It records requested live peak (conservative realloc overlap), cumulative requested bytes, Ready retention and after-drop retention before output serialization. It is not RSS or a performance benchmark. The initial exploratory log used output serialization inside the retention interval; use only the corrected baseline for retention conclusions.

Required closure controls:

- Cold and warm original environments; zero/tiny/just-under/exact/generous allowances. A low allowance may return a typed limit, but must not start large unreserved work. A funded call must retain its original semantic expectations.
- Width, exact long member names, enum/literal byte volume, numeric operands, pattern source/form expansion, many distinct property patterns, and same-input retries.
- Diamond annotation graphs and multiple unevaluated holders; increase stack independently to prevent accidental protection by a different limit. Include ordinary and bounds-program builds.
- Pattern compiler failure followed by sharing/rebuild; missing-resource stand-in reconstruction; a compiler refusal followed by semantic fallback. The same outer allowance must pay for every attempt.
- Long original pointers on errors and cancellation: bounded/omitted provenance, no diagnostic payload copy after exhaustion.
- Cancellation at every exposed checkpoint, post-build cancellation dropping the candidate, a clean fresh-control retry, sticky overflow/refusal and no partial successful cache publication.
- Instrumented cumulative reservation and actual requested allocations on the reviewed architectures; document fixed failure/control overhead separately from input-sized work. Warm caches only avoid charging storage/work truly reused; fresh compiler products still require reservations.
- Existing core/required evaluator/auxiliary semantics, consumer checks and matched performance gates stay unchanged. Capability/limit refusals are not semantic passes.

`source-hashes.json` and `locked-backend-source-hashes.json` pin the inspected source. Source snapshots and exact probe inputs are retained. The next independent gate is the concrete accounting implementation and these probes; this report makes no backend-budget PASS claim.

Standalone receipt probe reproduced all 86 observations successfully (`standalone-reproduction.json`). During that run the SDK lock gained independent carrier work; all inspected behavior files and the complete jsonschema/referencing/regex/serde_json package records remain unchanged. The receipt retains its own lock and both identities (`reproduction-identity.json`). No build is left running.
