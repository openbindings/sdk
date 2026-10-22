# OpenBindings JSON Schema evaluator

Validate values against an operation's input or output value contract. This
optional companion supports JSON Schema 2020-12, exact JSON numbers and explicit
immutable schema resources. Use it alongside `openbindings`.

```rust
use openbindings::{ValueOutcome, validate_document};
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let validation = validate_document(
        br#"{"openbindings":"0.2.0","operations":{"lookup":{"input":{"type":"string"}}}}"#,
    )?;
    if validation.conformant().is_none() {
        return Err(format!("document conformance: {:?}; findings: {:?}",
            validation.report().conclusion, validation.report().findings).into());
    }
    let proof = validation.conformant().ok_or("document is not conformant")?;
    let operation = proof.resolve_operation("lookup").ok_or("operation is missing")?;
    let input = operation
        .input()
        .ok_or("operation has no input contract")?
        .prepare(&DefaultEvaluator::new())?;
    match input.validate_json(r#""item""#)? {
        ValueOutcome::Satisfies => println!("input satisfies the contract"),
        ValueOutcome::Fails { message, problems, problems_complete } => {
            println!("{message}: {problems:?}; diagnostics complete: {problems_complete}");
        }
        ValueOutcome::Undefined { detail } => {
            println!("undefined result: {}", detail.message);
        }
        ValueOutcome::NoVerdict { detail } => {
            println!("no verdict ({:?}): {}", detail.reason, detail.message);
        }
    }
    Ok(())
}
```

`prepare(&DefaultEvaluator::new())` uses `Limits::default()` and no supplied
resources. A missing operation and an absent contract are `None`;
preparation refusal is a `NoVerdict` error that judges no instance. Ready means
prepared, not decidable. `validate_json` admits exact text or bytes under the
default JSON limits; `Err` means admission failed and `Ok` contains any of the
four value outcomes. Duplicate members yield no-verdict after admission.

Configure the evaluator or supply resources with `prepare_with`. The evaluator is
borrowed for the call, and one `ResourceSet` can serve many preparations:

```rust,edition2024
use openbindings::{NoVerdict, PreparedValueContract, ResourceSet, ValueContract};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn with_resources(
    contract: ValueContract<'_>,
    resources: &ResourceSet,
) -> Result<PreparedValueContract, NoVerdict> {
    let evaluator = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: Some(10_000_000),
        ..Limits::default()
    });
    contract.prepare_with(&evaluator, resources)
}
```

Use `JsonValue::parse_with_limits` for custom admission limits, or
`JsonValue::from_serializable` for ordinary Rust values. Keep admitted exact values
and call `PreparedValueContract::validate` to check them repeatedly without reparsing.

Retain and clone the prepared value contract for repeated validation. Each `prepare`
call compiles independently. The packaged [first-use example](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/first_use.rs)
adds aliases and exact wide integers; the [replacement example](https://github.com/openbindings/sdk/blob/main/crates/openbindings-json-schema-evaluator/examples/replacement.rs)
keeps old work usable while installing a new document and resource set.

## Limits

The evaluator never fetches references. Supply resources through `ResourceSet`;
new resources require a new preparation. A contract that reaches a resource that
was not supplied still prepares, with an empty stand-in for it: a pass that
reaches the stand-in gives no verdict (`resource-unavailable`) naming the
missing resource, and a value whose evaluation never reaches it keeps its
verdict, which then cannot depend on the missing content. One keyword decides
without evaluating its sources: `unevaluatedProperties` and `unevaluatedItems`
read the structure of the schemas that apply to the same instance to collect
their annotations, so a contract in which those sources can reach a stand-in
refuses preparation (`resource-unavailable`, naming the missing resource).
Preparation also refuses a reference to a fragment of a resource that was not
supplied, or a reference that identifies no schema (an absent pointer target or
plain-name anchor, or a malformed fragment), with a `NoVerdict` that names the
schema holding the reference.
As conservative preparation, it also refuses a contract that reaches a pattern
invalid as an ECMA-262 regular expression with Unicode semantics
(`schema-pattern-compilation`), or a schema that could apply again without
consuming the instance, such as one whose `$ref` names itself (`in-place-cycle`). The default evaluator never returns
`ValueOutcome::Undefined`, which custom evaluators may establish. A prepared
value contract does not promise a verdict for every value. Handle no-verdict
during both preparation and validation.

The evaluator bounds what the SDK itself reads, writes and recurses through.
It does not estimate the memory the JSON Schema library's compiled program
takes. `Limits` holds the configurable bounds. A limit that protects the
process has a default; `evaluation_steps`, which bounds only time, is the
application's to set and is off unless it does, and so is `schema_resources`,
which bounds the time and memory of compiling a contract that reaches many
resources:

- `evaluation_steps` sets no cap by default, so a pass takes time that grows
  with the schema and the value, and can grow exponentially with the schema:
  30 levels of `anyOf` that each name the next level twice, ending in
  `false`, take about 2.5 s to check `{}`. `Some(n)` charges one unit for
  each schema a reference enters and for each element, member and literal
  comparison step, and gives `evaluation-work-limit` once a pass spends `n`;
  an application that evaluates untrusted contracts or values within a
  deadline sets one.
- `stack_bytes` (256 KiB) is the stack the thread that prepares and evaluates
  has. The evaluator sizes three bounds from it, each with a stack cost
  measured in release builds on Linux, macOS and Windows. A value is evaluated
  only if the schema evaluations it could nest fit 256 bytes of the stack each
  (1,024 at the default): preparation bounds the schema objects any evaluation
  path can hold as a function of the value's depth, and a value whose bound
  exceeds that gives `evaluation-depth-limit` before it is evaluated. The
  bound is static: a deep contract whose paths only some values follow refuses
  the others too. The library's `unevaluatedProperties` and `unevaluatedItems`
  validators recurse in ways that bound does not model, so a contract holding
  one is also guarded as it evaluates: every read of a child instance compares
  the stack address it runs at with the pass's start, and a pass that has used
  more than half the stack (128 KiB at the default) stops descending and gives
  `evaluation-depth-limit`. And preparation refuses an unevaluated keyword's
  annotation walk weighing more than the stack admits (below). Each refusal
  states its arithmetic and names `stack_bytes`. Release builds prepare, check
  and explain the 39 recursive shapes `tests/evaluation_stack.rs` covers on
  threads of 192 KiB, 256 KiB and 1 MiB with the evaluator sized for each: a
  larger stack admits deeper values of every shape, and a smaller one refuses
  them sooner. The other recursion preparation does is bounded by the
  input-shaped limits below and core's admission limits, whose defaults fit
  the default stack; a host giving a smaller stack lowers those too.
- `compile_json_depth` (512) bounds the projected JSON nesting compiled.
- `pattern_bytes` (1 MiB) bounds each pattern, and its form for the
  linear-time engine, which spells each set out (a `\S` takes 191
  bytes): a longer form gives `pattern-size-limit` without being written out.
  `pattern_depth` (32 groups) bounds each pattern's nesting, and release builds
  prepare and check patterns at that depth on a 256 KiB thread (debug builds
  need more; see
  [support](https://github.com/openbindings/sdk/blob/main/docs/support.md)).
  `total_pattern_bytes` (4 MiB) bounds the forms the evaluator keeps for a
  contract's patterns together: each distinct `pattern` form once, and the form
  of each `patternProperties` name. A form spells every set out, so it can be
  far longer than its pattern (`\S` takes 191 bytes); preparation counts each as
  it is written and refuses at the first that passes the limit
  (`pattern-total-byte-limit`).
- `total_pattern_memory` (64 MiB) bounds the linear-time engine's memory for
  a contract's compiled patterns together: each distinct `pattern` form the
  run matcher does not match, and each `patternProperties` name, built through
  the regex crate's two size settings. A quarter of it, at most 2 MiB each, is
  their search caches (`dfa_size_limit`), in equal parts; a cache bounds only
  how fast a search runs. Each form first compiles within 48 KiB
  (`size_limit`), which all but one of 5,389 measured real patterns fit, so a
  contract compiling more patterns than the rest holds at 48 KiB each (1,024 at
  the default) refuses at the first past that count, before compiling any. The
  forms that need more share what is left equally; those that need less than
  their part keep only what they need, to within a sixteenth, and the rest is
  shared again among the others, at most 32 MiB each. So a pattern's outcome
  depends on the small patterns elsewhere in its contract only through their
  48 KiB each, and never on the order strings reach the patterns: a single
  `^.{1,10000}$` (about 13.2 MiB) compiles beside up to 743 small patterns,
  four of them compile together, and five need more than is left. The first
  time a form is found that does not fit its 48 KiB, the contract's compiled
  patterns not yet compiled are compiled then. A `patternProperties` name that
  does not fit what it is given refuses preparation, and a `pattern` value,
  compiled the first time a string reaches it, gives that string no verdict;
  both use `pattern-total-memory-limit`, whose message states the arithmetic,
  or `pattern-size-limit` when the form needs more than 32 MiB. Refusing forms
  that together need more than is left takes building about that much: 40
  names of the form `^.{1,10000}N$` refuse in about 0.2 s, with about 2 MB of
  heap at their peak.
- `program_bytes` (48 MiB) bounds how much the SDK's program for one contract
  may grow beyond its projected schemas. Dynamic specialization copies
  a schema reached through `$dynamicRef` once for each dynamic context it is
  reached in. Preparation estimates the memory each copy takes as the SDK holds
  it (160 bytes for the definition, 640 for each object's map, 56 for each
  member, 32 for each array element and a byte for each byte of its strings and
  member names; ordinary definitions measure at most a sixth below the estimate)
  and each context (64 bytes and 8 for each dynamic-scope entry), and refuses a
  contract whose copies and contexts would pass the projected schemas' own
  estimate by more than `program_bytes` with `program-specialization-limit`,
  whose message states the arithmetic. It counts the SDK's own program, not the
  memory of the library's compiled program.
- `schema_resources` sets no cap by default; `Some(n)` refuses a contract
  reaching more than `n` schema resources (the document resource holding its
  schema and each embedded `$id`, supplied and packaged meta-schema resource)
  with `LimitExceeded` and `schema-resource-limit` before compiling it
  (`limits: { schemaResources: n }` in TypeScript). Preparation time grows
  linearly with the resources a contract reaches: 10,000 supplied resources
  take 0.22 s in the reference build. The refusal comes after core projects
  the contract, which its admission limits and 100,000 schema positions bound,
  and before the evaluator copies the resources and the library compiles them,
  so it bounds the time and memory of that compilation: a 7 MB document whose
  input reaches a chain of 99,990 `$id` resources prepared in 11.5 s at a
  723 MB peak without a cap, and refused in 1.2 s at a 502 MB peak with a cap
  of 1,000 (release build on macOS arm64, peak resident memory of the whole
  process).

Core's document admission limits bound the bytes and nodes of every document
and resource, and its projection refuses a contract reaching more than 100,000
schema positions (`schema-node-limit`); that bound is core's, shared with its
own fixed checks. The library compiles an unevaluated keyword's annotation
sources by recursion, so preparation weighs the walk and refuses one weighing
more than `stack_bytes` admits (`unevaluated-walk-limit`, at that keyword): a
fifth of the stack is left to the caller, 99 KiB to the rest of preparation,
and each unit takes 1 KiB, so 105 at the default. A reference weighs 4, each
`allOf`, `anyOf` or `oneOf` on its path 5 more, each `if`, `then`, `else` or
`dependentSchemas` 7 more and reaching another holder 1 more, so each unit
takes at most a KiB on every host measured. The default admits 26 direct
`$ref`s (21 with a holder at every level), 8 levels of `allOf: [{"$ref": ...}]`
inheritance (7), or 7 levels through `then`, `else` or `dependentSchemas` (6);
512 KiB admits 77 `$ref`s and 23 levels of inheritance, and 192 KiB 13 and 4.
Release builds on Linux, macOS and Windows prepare the longest admitted walk of
each kind with a fifth of a thread of each of those sizes already in use.

Within those bounds, preparing a schema can still take more memory than a
host may want, since the library's compiled program grows with what it
compiles: 1,000 holders of
`unevaluatedProperties` that each inherit a base schema of 4,000 properties
peak at 400 MB, and a dynamically specialized contract whose 4,096 contexts
take 44.8 MiB of the default `program_bytes` peaks at 158 MB. A host that prepares schemas from untrusted parties should
prepare them in an isolated worker or process with an operating-system memory
limit. Values checked against trusted schemas stay bounded by
`stack_bytes` and, when the host sets one, by `evaluation_steps`.

Two further cases give no verdict whatever `Limits` sets. Exact `multipleOf`
divides within a fixed arithmetic bound: when the value or the divisor has
more than 4,096 significant digits, or an exponent beyond ±20,000 once the
trailing zeros of its digits are counted in it, a division the answer needs
gives no verdict (`LimitExceeded`, `numeric-arithmetic-limit`). Other numeric
keywords compare exactly at any size. And values keep unpaired UTF-16 units:
keywords that compare values exactly, such as `const`, `enum`, `required` and
`uniqueItems`, decide them, while a keyword that reads a string or member name
as Unicode text, such as `minLength`, `maxLength`, `pattern`, `properties`,
`patternProperties` or `propertyNames`, gives no verdict
(`UnsupportedCapability`, `lone-surrogate-instance`) when it reaches one.

Patterns are ECMA-262 patterns with Unicode semantics, matched by a linear-time
engine, so a match takes time linear in the string. The word boundary `\b`
matches against ECMA-262's ASCII word characters. A pattern that needs a
backtracking engine (lookaround or a backreference), has a Unicode property
escape, or holds the non-word boundary `\B` (which the engine's `is_match` can
answer wrongly beside multi-byte characters) is not matched: a string that
reaches it in `pattern` gives no verdict (`pattern-backtracking`,
`unicode-property-matching` or `pattern-non-word-boundary`) located at that
`pattern` keyword's original schema location, and as a `patternProperties` name
it refuses preparation. A pattern whose form needs
more of `total_pattern_memory` than it is given gives
`pattern-total-memory-limit`, or `pattern-size-limit` past 32 MiB. The
reading is meant to match as
ECMA-262 does, and a differential against V8 checks it through both keywords,
but it is a translation, not an ECMA-262 engine: it writes every set as the
positive class of the scalar values it matches, since the engine's negation of a
class whose ranges meet across the surrogate gap adds a range across it.
`format` is an annotation; another declared schema dialect is not treated as
2020-12.

A `$dynamicRef` resolves through the dynamic scope of the path that reaches it,
also when a `$ref` cycle re-enters a resource that declares a dynamic anchor:
preparation compiles a schema once for each dynamic context it is reached in,
within `program_bytes`: the 256 dynamic contexts of 8 levels that each choose
between two paths take about 6.8 MiB, and the 2,048 of 11 levels about
70 MiB, more than the default 48 MiB. Otherwise preparation compiles each
reached schema once, in time and memory that grow with the contract, except
that the library takes time quadratic in the length of a long same-document
reference chain. See
[measured behavior](https://github.com/openbindings/sdk/blob/main/docs/performance-boundaries.md).

Each call chooses its detail. `validate` is the cheap check: an established
failure has a fixed message, an empty problem list and `problems_complete =
false`. `explain` returns the library's first error for the same verdict, with
a fixed message for its kind. Building that error allocates in proportion to
the schema and value and takes about twice a check's stack per level, so a
value nested deeper than half the nesting `stack_bytes` admits is reported as
failing without its problem; keep `validate` on paths that check untrusted input. The
explained problem names the failing node by its exact pointer from the value
passed (`instance`) and the failing keyword by resource and exact pointer
(`schema`). `JsonValue::locate`, `Conformant::locate` and `ResourceSet::locate`
find line and column on demand, so an explanation costs no position. A prepared
value contract keeps only the original locations of the schemas it reached,
never their text, the document, its supplied resources or any position.
Missing or incomplete diagnostics never weaken an established failure.

Cancellation uses `WorkControl` and is cooperative. Cancelled preparation produces
no prepared owner; cancelled validation leaves a prepared owner usable for a later
attempt. Use a fresh control for an independent retry.

This unpublished package requires Rust 1.88. See
[support](https://github.com/openbindings/sdk/blob/main/docs/support.md) for stack
qualifications and [custom evaluators](https://github.com/openbindings/sdk/blob/main/docs/schema-evaluators.md)
for the integration contract.

## Reference

The crate reference documents `DefaultEvaluator` and `Limits`.
[Resolver and evaluation maintenance](https://github.com/openbindings/sdk/blob/main/docs/maintainers.md)
links the resource-identity rules and the
[dependency maintenance contract](https://github.com/openbindings/sdk/blob/main/DEPENDENCY-MAINTENANCE.md),
which describes how the evaluator composes the unmodified JSON Schema library.
