# OpenBindings JSON Schema evaluator

An optional evaluator for operation input and output schemas. Core owns document
conformance, original schema identity and reference resolution; this companion
implements per-value evaluation with exact numbers and explicit external schemas.
It never fetches resources. This unpublished package requires Rust 1.88.

```rust
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let checked = validate_document(
        br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
    )?;
    let proof = checked.conformant().ok_or("document is not conformant")?;
    let schema = proof.resolve_operation("lookup")
        .ok_or("operation is missing")?.input().ok_or("input schema is absent")?;
    let prepared = schema.prepare(&DefaultEvaluator::new(), &ExternalSchemas::default())?;
    let value = JsonValue::parse("7")?;
    match prepared.explain(&value) {
        ValueOutcome::Satisfies => println!("satisfies"),
        ValueOutcome::Fails { problems, problems_complete, .. } => {
            println!("fails; diagnostics complete: {problems_complete}");
            for problem in &problems {
                println!("instance={:?} code={} details={}",
                    problem.instance.to_string(), problem.code,
                    serde_json::to_string(&problem.details)?);
            }
        }
        ValueOutcome::Undefined { detail } => println!("undefined: {}", detail.message),
        ValueOutcome::NoVerdict { detail } => println!("no verdict: {}", detail.message),
    }
    Ok(())
}
```

`evaluate` is the yes/no check whose failure carries no problems;
use `explain` when you need failure diagnostics.

`Schema::prepare(&evaluator, &external)` uses default `PreparationLimits` and no
cancellation and returns `Result<PreparedSchema<DefaultValidator>, PreparationFailure>`.
Use `prepare_with_control(&evaluator, &external, &limits, control)` for explicit
preparation limits or cancellation. A preparation failure judges no value. An absent operation or absent input/output
schema is a separate `None`. A prepared schema owns its evaluation state and can
outlive the document and resource handles. Clone it for repeated evaluation;
preparing again performs a new preparation. Supplying different external content
requires a new preparation.

Parse exact text or bytes with `JsonValue::parse` or `parse_with_limits`, then call
`PreparedSchema::evaluate` or `explain`. Parsing failure is an error distinct
from the four value outcomes; retain the admitted value for repeated checks.
Duplicate instance member names produce `NoVerdict` after parsing.

## Execution paths

Every prepared validator retains an Original semantic plan. At default evaluator
and `PreparationLimits` settings, an eligible subset also gets a projected schema
compiled by jsonschema. `evaluate` and `explain` first try that compiled lane;
diagnostics use Original after failure. Exhausted diagnostic work leaves the established failure
intact with incomplete problems. Changing any limit from its default
selects Original, even when the new limit is larger. Route selection is private.

Compiled admission requires established static references, standard 2020-12
interpretation and supported patterns. Dynamic references, unevaluated keywords,
foreign/custom dialects and unresolved effects select Original. Acyclic expansion
is capped at 100,000 applications; supported productive recursion has a separate
finite-expansion proof. Value admission checks exact-text compatibility, static
stack depth, arithmetic/scratch bounds and a weighted schema-application count
times value nodes of at most 40,000,000. A value outside that profile uses Original;
selection alone is not a refusal. Failure during compiled construction can still
refuse preparation.

Original keeps enum query counts inside each evaluation, keyed by schema
position. Enums of eight choices or fewer always scan to the first match and
allocate no index. For a larger enum with `m` choices, the first `ilog2(m)`
queries also scan; the next query builds a fallible heapsort index, then uses
binary search for the rest of that evaluation. A single query allocates nothing
new and never costs more SDK work than the former complete scan. Preparation
builds no enum membership index or counter storage. Neither enters the prepared
plan or survives a value call, so cold and warm calls spend the same work.

The evaluation ledger reserves four bytes per indexed choice, plus accounted
entries for additional enum nodes. An index of 20,000 choices uses 80,000 bytes.
Exact comparisons release their decimal and traversal scratch reservations when
they finish. The retained index stays charged, and a comparison whose scratch
exceeds the available allowance still returns `semantic-analysis-memory-limit`.
Equality remains exact for mathematical numbers, decoded strings and objects in any member order. Work is
deterministic on wasm32 and 64-bit hosts. The compiled library lane and admission
policy are unchanged.

Original `uniqueItems` keeps sorted fingerprints for candidate selection. Groups
of at most eight candidates retain pairwise exact verification. Larger groups
use the same fallible heapsort and exact JSON order as enum membership, then
check adjacent values. Verification is worst-case `g log g` comparisons for a
colliding group of `g` items, without relying on a secret seed on wasm32.
Large groups reserve four bytes per item for sorted positions, keeping the
ordinary fingerprint buffer read-only. Positions, comparisons and heap frames
charge the evaluation ledger. Values without collisions retain their previous
work debits. Exact numbers, decoded UTF-16 strings, arrays and unordered member
maps have the same equality relation as `literal_equal`.

Original uses heap frames, cooperative work checks and SDK storage reservations,
releasing completed exact-comparison scratch. Compiled evaluation recurses in the
library, with static admission and a conservative peak scratch estimate within
the default 48 MiB allowance.
Evaluation work accounting uses fixed-width arithmetic on native and Wasm.
Without a requested work cap it cannot report a work limit from counter overflow;
caps representable on both targets stop at the same debit. Existing cumulative cap
types and the TypeScript facade's 32-bit cap range are unchanged.
It checks cancellation before and after the library call, not inside that call.
The application-count ceiling is not a CPU deadline. Neither path promises total
heap, process RSS or host-stack bounds. Hard interruption requires an isolated
Worker or process with a responsive supervisor. Both paths retain exact primitive
semantics; they may differ in ability to finish within their resource policies.

## Results and semantic dependence

`Satisfies` and `Fails` are established validity results. `Undefined` means validity
depends on a known undefined result under OpenBindings §5.5, with coherent opposing
interpretations. Unsupported features, unavailable resources, exhausted budgets
and cancellation alone do not establish undefinedness.

The Original plan reasons over original schema locations, combining
predicate results with evaluated-property and evaluated-item annotations. Repeated
original predicates share the same exact JSON value identity; pattern predicates
share the same original site and tested UTF-16 text. Numeric spellings and object
member order do not split equal values. Actual occurrence identity remains separate
for recursion detection.

Malformed local reference operations and invalid Unicode patterns do not create
annotations. Known applicators produce annotations only under their corresponding
successful interpretations. Unavailable content may supply both assertions and
annotations, and may refer to identified predicates already present in the
document. The dependence proof accounts for those correlations. The anonymous OBI
document resource is unnameable from external content; its private predicates have
a different independence boundary. A conservative opaque local summary may depend
on either category.

Pure reference aliases preserve resource entry and dynamic scope before being
normalized. A pure alias cycle can establish `Undefined`. Recursive summaries also
refine separately proved validity and evaluated-property/item coordinates between
attempts. A completed state is cached only when every coordinate is independent
of transient cuts. A bounded original-graph producer analysis prevents recursion
from inventing evaluated names or positions. Complete producer states supply
formula-valued upper bounds, preserving conditional selection and undefined
pattern correlations; unknown contents keep their support unknown. Exact property
names remain UTF-16 sequences even in supplied schemas. A malformed reference
contributes a local undefined result. An identifier establishes a base rather
than an assertion: a malformed identifier affects validity through references
that depend on that base, not merely by being present.

After universal S/F proofs, a separate direct-entry certificate can establish U
for a canonical negating self-cycle or a fully closed, strongly connected OR
component. It inspects every original transparent wrapper, accepts only standard
2020-12 interpretation with no effective dynamic scope, and requires complete
closure. It does not solve general Boolean fixed points or publish reusable
choices for individual component members. Unfinished proofs and excluded recursive
shapes remain honest `NoVerdict` results. All rounds, closures and retained facts
share the same work and allocation allowance.

The 1,546 required migration values pass, including standard external dialects and
custom vocabulary/format cases. The additional recursive extension has 33
independently adjudicated controls. Finite suites and these deliberately narrow
certificates do not establish complete JSON Schema support.

## Limits and cancellation

```rust,edition2024
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator, Limits};

fn prepare_bounded(schema: &Schema, resources: &ExternalSchemas)
    -> Result<PreparedSchema<DefaultValidator>, PreparationFailure>
{
    let evaluator = DefaultEvaluator::with_limits(Limits {
        evaluation_sdk_work_units: Some(10_000_000),
        schema_resources: Some(1_000),
        ..Limits::default()
    });
    schema.prepare(&evaluator, resources)
}
```

| Limit | Default | What it bounds |
| --- | --- | --- |
| `evaluation_sdk_work_units` | no cap | Cooperative SDK evaluation work units |
| `pattern_search_work_units` | 250,000,000 | SDK work charged to one uncancellable Original pattern search; `None` removes the ceiling |
| `schema_resources` | no cap | Reached original resource construction |
| `pattern_bytes` | 1 MiB | One pattern source and its expanded form |
| `pattern_depth` | 32 | Pattern group nesting |
| `total_pattern_bytes` | 4 MiB | Unique normalized pattern forms across values and property names |
| `total_pattern_memory` | 64 MiB | Immutable pattern-program representation |
| `evaluation_sdk_reservation_bytes` | 48 MiB | Original SDK storage reservations, with completed comparison scratch released; compiled peak scratch admission at defaults |

Each Original value invocation starts a fresh evaluation allowance. Verdict and diagnostic
passes share what remains; diagnostic exhaustion preserves an established failure
with incomplete problems. Required custom-meta evaluation debits both its own
evaluation allowance and the enclosing preparation Attempt, so either exhaustion
can return a preparation Limit. Reservation bytes are SDK estimates, not exact
allocator requests, live heap or immutable pattern-program size.

`PreparationLimits` bounds the original schema environment's footprint, cooperative
SDK work (`max_sdk_work_units`) and explicit SDK storage reservations
(`max_sdk_reservation_bytes`, 512 MiB by default). `max_sdk_work_units` is
optional and off by default (`None`). A time bound is the caller's choice through
`WorkControl` cancellation or `Some(work_units)`; the dated
[host measurement](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md#limits-principle-november-2026)
gives a work-unit suggestion for about 250 ms on that host. A local
undefined operation cannot bypass construction accounting: the semantic path
walks known original references and applicators, counts resources and checks
pattern budgets before becoming ready. Proof work, exact effect keys, annotations
and decision-graph storage are charged, including custom meta-schema admission
and retries after discovering a recursive component. Supported pattern matching borrows the input string;
unknown matches reserve their exact text-key allowance before constructing it.

Original pattern search checks cancellation before and after the matcher, never
inside it. `pattern_search_work_units` refuses a search before entry when its
conservative charge exceeds the ceiling. Finite anchored searches charge only
the bytes the matcher can inspect. Unanchored searches may prove a match in a
bounded prefix, keeping the complete haystack for assertions; an inconclusive
prefix returns the same limit code. See the [engine bound](PATTERN-WORK.md).
The result is `NoVerdict` with reason `LimitExceeded` and code
`pattern-search-work-limit`, distinct from the caller's cumulative
`evaluation-work-limit`. Diagnostics contain no pattern or instance text.
Raise the field or use `None` to remove the ceiling; either selects Original.
Explicitly supplying the default keeps existing lane selection. Compiled work
retains its own admission policy. Explanation preserves its established verdict
when Original diagnostic work is exhausted. This default treats uncancellable
work like storage: the caller cannot stop it during the call. It is a work bound, not an
elapsed-time ceiling; see the [measurements](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md#uncancellable-pattern-searches-november-2026).

Original evaluation work covers traversal, predicate work and nontrivial logical reasoning.
Simple Boolean identities reuse an existing result without another proof-work
unit. Growing annotation and match-count loops keep explicit cooperative checks;
a fast result never clears an exhausted shared preparation attempt.

Storage reservations bound conservative estimates of storage held at once.
Preparation charges collection capacities, owned pattern programs and cold fixed
compiler state. Shared owners are charged once. Fixed dependency construction
uses a version-bound profile qualified against requested live heap; this excludes
allocator metadata, stack and RSS. An existing const payload is not copied by
fixed syntax admission and has no source-sized syntax storage charge.
Local syntax checks, traversal scratch and nested meta-evaluation scratch release
reservations when dropped; retained programs and indexes stay charged. Mixed
construction estimates conservatively stay charged with their owner.
These counters cover the SDK sites described above, including temporary and failed
SDK construction while held; they do not count every dependency operation, allocator request
or custom callback. Dependency construction has separate structural and retained
representation limits, with measured resource qualification on supported builds.
Patterns are prepared once; equivalent normalized forms share an immutable
program, while search caches belong to individual value calls. Algorithm and
allocation checks live in the evaluator tests and `tools/diagnostic-memory-probe`.
Final matched-toolchain performance qualification remains part of the migration.

Symbolic reasoning supports at most 64 distinct effects in one proof, including
selected meta-schema and diagnostic passes. This is separate from concrete schema
or instance depth. Original also samples stack use against a private
256 KiB guard; this is an additional refusal check, not a guarantee about a host's
available stack. The former compiler-depth and stack-byte options are removed and
rejected as unknown settings. Build-specific physical-stack qualification remains
part of the current alignment checkpoint.

Cancellation is cooperative through `WorkControl`. Cancelled preparation produces
no prepared owner; a cancelled evaluation does not poison cached predicates or
prevent a later uncancelled retry. A smaller budget can replace an answer with an
honest inability, but cannot manufacture `Undefined` or reverse validity.

`tests/cancellation_work.rs` checks large-schema preparation and pattern-free
value evaluation through public controls. After the control first reports
cancellation, at most four additional polls may occur, independent of input size.
This bounds propagation at cooperative boundaries, not work between polls or
synchronous dependency execution.

## Patterns, exact values and diagnostics

Patterns follow ECMA-262 11th edition with Unicode semantics. The linear matcher
supports the translated subset, including the qualified Unicode property tables
and ASCII word boundary. Unsupported lookaround/backreferences, missing property
tables and non-word-boundary assertions retain explicit capability refusals when
validity depends on them. An invalid expression instead supplies a known undefined
match. `patternProperties`, `additionalProperties` and unevaluated keywords use
that same match choice coherently.

Numbers are compared exactly, including large integers and exponents. Expensive
numeric arithmetic can hit its explicit limit. JSON strings retain unpaired
UTF-16 units; exact equality preserves them, while unsupported Unicode-text
operations report their limitation. `format` is an annotation where its assertion
is optional. Required format assertion supports all 19 standard attributes;
unknown required attributes refuse preparation. Its `regex` attribute checks
ECMA-262 11th-edition no-flags syntax with Annex B, without executing a pattern.
An external resource's declared dialect is never silently read as 2020-12.
See [evaluator profiles and preparation](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md) for
custom meta-schema admission and the structural and cooperative resource limits.

An established failure stops further outcome-only child traversal. Reference-bearing item traversal cannot turn that failure
into a resource no-verdict. During explanation, root assertions are materialized
before further child traversal; if that traversal exhausts the allowance, those
problems survive with incomplete diagnostics. Once 100 problems are retained,
further child traversal stops with incomplete diagnostics. The same cumulative work and
reservation limits still apply, including to materializing those problems.

`evaluate` is the cheap check: failure has no problems and marks diagnostics
incomplete. `explain` preserves the same result and reports available bounded
problems at original source locations. Some semantic failures currently have no
specific diagnostic evidence and retain `problems_complete = false`. The
[API reference](https://github.com/openbindings/sdk/blob/main/docs/api-reference.md) describes the 100-problem cap, ordering,
and applicator reporting rules. `details` includes missing required member names,
expected types, exact numeric and size bounds, and complete enum choices from
the original schema. Bounds and choices retain original JSON token text. Tied
missing-member problems keep required-array order. Keywords without a matching
variant, including `const`, `pattern`, `multipleOf` and `additionalProperties`,
have no details. Facts count against the diagnostic storage allowance. A fact
that cannot fit or be represented exactly is omitted and leaves
`problems_complete` false; enum lists are never partial.

Messages stay fixed SDK text and `Debug` redacts facts. Rejected instance values
are never copied into facts or messages. Serializing whole problems into a public
response sends schema facts. An app with a private contract should forward only
`instance` and `code`. Detail work never retracts an established result. Oversized origins are omitted
before copying them, pattern diagnostics do not echo authored regex text, and an
unavailable resource has no invented source coordinate.

Prepared validators retain an immutable original schema environment, prepared
primitive operands and shared pattern programs, plus backend state when compiled. Each call owns its mutable proof
state and search caches. No caller borrow is retained.
See [custom evaluators](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md),
[first use](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/first_use.rs), and [replacement](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/replacement.rs).

`PreparedSchema::retained_bytes()` calls the optional `SchemaValidator` method.
`DefaultValidator` returns `Some` on the SDK lane, counting the retained original
environment, complete source snapshots, supplied resources, caches, plan maps,
exact operands and pattern representations. It returns `None` when it retains
a compiled `jsonschema` validator: the library does not expose its allocation
capacities, and the SDK does not model its internals. Configuring
`Limits::evaluation_sdk_work_units: Some(...)` selects the SDK lane.
The query uses existing capacities and estimated collection occupancy; regex
NFAs expose representation size. Process-wide packaged schemas and fixed format
validators, allocator overhead and Wasm memory growth are excluded. Shared
allocations are counted per owner, so shared clones report the same estimate.
This is neither resident memory nor what dropping one handle frees. Only the
query walks storage; preparation and evaluation do no accounting. Later shared
cache growth can change the result. Custom validators default to `None`.
