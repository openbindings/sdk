# Capabilities and limits

Use this SDK to read, validate, author and inspect OpenBindings interface
documents, and to validate caller-facing values against their value contracts.
Start with a working example in the [Rust guide](docs/rust-first-use.md) or
[TypeScript guide](packages/typescript/README.md).

## Documents and exact values

Core checks OpenBindings 0.2 document rules and reports met, violated or
inconclusive evidence where applicable. A parsed document keeps its exact bytes
and can be edited in place; validation that establishes every rule yields a
conformant document. Unsupported specification versions
are refused separately. A document's conformance does not establish its author
claims about examples, bindings, availability or dependencies.

Exact JSON retains numeric tokens, escaped UTF-16, duplicate-member evidence,
unknown fields and original source locations. Typed authoring preserves opaque
values while re-encoding typed fields. Checked conversion of ordinary application
data refuses unsupported or inexact representations. Conversion cannot restore
precision already lost by the application. Document conformance rejects an escaped
unpaired surrogate in any member name or string value as OBI-01, including opaque
content and embedded schemas. OBI-02 through OBI-13 then do not apply. Escaped
surrogate pairs and noncharacters remain permitted. Draft inspection, caller
values and supplied schema resources still retain unpaired UTF-16 units exactly,
and so do the JSON Pointers that locate their nodes.

Default parsing admits 32 MiB, 10,000 container levels and 1,000,000 nodes.
`validate_document` admits the same bytes and nodes at any nesting. These
admission limits are a setting at `validate_document_with_limits`,
`Document::parse_with_limits` and `JsonValue::parse_with_limits` (`JsonLimits`),
and at TypeScript's `validateDocument`, `Document.parse` and `parseJson`
(`limits`). `Document::new`, `Document::from_json`, `JsonValue::parse`,
`JsonValue::string`, `Publication::from_bytes` and `validate_json` use the
defaults, as do TypeScript's `Document.create()`, `ExactJson.from`,
`validateJson` and a prepared value contract's `validate` and `explain` of an ordinary
value.
A document keeps the limits it was admitted under, and its edits apply them to
the edited document. Discovery raises only the byte limit, to its configured
body bound; a body past the default nodes comes back undetermined and is admitted
by validating it again with `validate_document_with_limits`. Value-contract
preparation keeps fixed bounds of its own, which no setting raises. Every
admission and edit limit refusal names how to admit more, including the route
for calls that take no limits.
Ordinary Rust Serde conversion has a 128-container ceiling. Document reports retain at
most 4,096 findings and 8 MiB of pointers. Truncation leaves rule evidence
intact. Document validation's memory grows linearly with the document: its copy
of the text, the decoded text of its escaped strings, its tree, and validation
state per schema position, `$id` declaration and retained finding. Compared
identifiers share runs of the `$id` texts, so a long or deeply nested `$id`
costs memory and time linear in the document. At the default limits the
diagnostic memory probe validates 34 documents, most filling the byte or node
limit, with structures that grow these terms, and fails if one allocates more
than 300 MiB beyond the caller's input, counting every reallocation as holding
its old and new blocks at once. The largest, a third of a million escaped
`$id`s, each with its own authority, a query, a fragment and a dot segment that
makes its path a copy, allocates 256.0 MiB, against 364.9 MiB at the former
64 MiB default. A caller that hands over its buffer, through
`validate_document_owned`, `Document::parse_owned`, their `_with_limits` forms
or `JsonValue::parse_owned_with_limits`, gives the document or value that
buffer as its text, saving the copy: the same document then allocates
224.3 MiB beyond the buffer. The TypeScript package always hands over the
module's copy of its input. See
[performance and allocation boundaries](docs/performance-boundaries.md).
OBI-02's whole-document check and each embedded schema's OBI-10 meta-schema
check run on the unmodified jsonschema 0.58.6 library with its default pattern
engine. Every fixed pattern is regular, so that engine matches it on a finite
automaton in time linear in the string, with the results of ECMAScript's `u`-mode
RegExp. The derived schema has no recursive reference and the meta-schema check
cuts its recursive edges, so every document the admission limits admit, default
or caller-selected, gets OBI-02 and OBI-10 verdicts, with work linear in those
limits and no work budget of the checks' own. A SemVer near miss, the costliest
input per byte, took about 0.35 seconds at the default 32 MiB limit, 0.6 seconds
at 60 MB and 0.7 seconds at 64 MiB in a release build on an Apple M1 Max; peak
memory was the input text and the stored document, and the checks' own memory
does not grow with the string. The admission byte limit is how a caller bounds that time:
`validate_document_with_limits` with a smaller `max_bytes` admits less. A
meta-schema check reads its schema's own members and only the type of each
subschema, so a node is read by at most two checks and their work together stays
linear in those limits. A failing schema's members are read once more to locate
OBI-10 findings at the keywords whose values fail.

## Value validation

Document validation needs no caller-supplied evaluator. Operation values require
an explicit Rust `SchemaEvaluator`; the TypeScript facade supplies the bundled
evaluator. Only a conformant operation hands out its input and output value
contracts, and preparing one borrows the evaluator and any supplied resources. Core alone makes no satisfies/fails decision, even for a boolean
schema. The [evaluator guide](docs/schema-evaluators.md) explains integration.

The optional default evaluator supports JSON Schema 2020-12 with exact decimal
comparison and explicit immutable resources, composed over the unmodified
jsonschema 0.58.6 library. It performs no reference acquisition. `format`
remains an annotation. Patterns are read as ECMAScript `u`-mode patterns,
the word boundary `\b` included, and translated for a linear-time engine; a
differential against V8 checks the translation through both pattern keywords.
One that needs a backtracking engine (lookaround or backreferences), applies
a Unicode property escape or holds the non-word boundary `\B` produces
no-verdict, as do other unsupported capabilities. A `$dynamicRef` resolves through the dynamic scope
of the path that reaches it, also through `$ref` cycles.

The evaluator bounds what the SDK itself reads, writes and recurses through; it
does not estimate the JSON Schema library's compiled memory. On by default:
core's document admission limits and its 100,000 schema positions per value contract;
the projected JSON nesting compiled (`compile_json_depth`, 512,
`compileJsonDepth` in TypeScript);
the stack the host gives (`stack_bytes`, 256 KiB, `stackBytes` in TypeScript),
from which the evaluator sizes evaluation depth (256 bytes for each nested
schema evaluation, 1,024 at the default), bounded from the value contract before a
value is evaluated, the stack a pass of a value contract holding
`unevaluatedProperties` or `unevaluatedItems` may use as it evaluates (half,
128 KiB at the default: a bound on the stack's bytes, so the depth it admits
varies with the build and the host) and the unevaluated walk limit below, so release
builds on Linux, macOS and Windows prepare, check and explain the 39 recursive
shapes the stack test covers on threads of 192 KiB, 256 KiB and 1 MiB with the
evaluator sized for each; each pattern's bytes, form and nesting (1 MiB,
32 groups) and the forms kept for all of a value contract's patterns together (4 MiB,
`totalPatternBytes` in TypeScript); the regex crate's memory for all of a
value contract's compiled patterns together (`total_pattern_memory`, 64 MiB,
`totalPatternMemory` in TypeScript, `pattern-total-memory-limit`, with a
message that states the arithmetic), spent through the crate's lazy-DFA cache
setting, a quarter of it in equal parts, and its compiled-size setting, within
which each pattern first compiles in 48 KiB while those that need more share
the rest, so small patterns beside a large one change its outcome only through
their 48 KiB each: `^.{1,10000}$` compiles beside up to 743 small patterns and
four such counted repetitions of about 13 MiB compile together, while 40
refuse in about 0.2 s; the
memory dynamic specialization adds to a value contract's program, estimated for the
SDK's own copies and dynamic contexts (`program_bytes`, 48 MiB, `programBytes`
in TypeScript, `program-specialization-limit`); and an unevaluated keyword's
annotation walk, which the library compiles by recursion, weighed by the kind
of each reference against what `stack_bytes` admits with a fifth of it left to
the caller (`unevaluated-walk-limit`: at the default 26 `$ref`s, 8 levels of
`allOf` inheritance, 7 through `then` or `dependentSchemas`). Each of these
refusals states its arithmetic and names the setting to raise. Off unless an
application sets them: an evaluation work cap (`evaluation_steps`), which bounds
only time and without which time can grow exponentially with some schemas, and a
cap on the schema resources one value contract may reach (`schema_resources`,
`schemaResources` in TypeScript). That cap refuses a value contract after core projects
it and before the evaluator copies its resources and the library compiles them,
so it bounds the time and memory of that compilation: a 7 MB document whose input
reaches a chain of 99,990 `$id` resources prepared in 11.5 s at a 723 MB peak
without it, and refused in 1.2 s at a 502 MB peak with it set to 1,000. Within
these bounds, preparing a schema can still take more memory than a host may
want, since the library's compiled program grows with what it compiles: 1,000
holders of `unevaluatedProperties` that each inherit 4,000 properties peak at
400 MB. A host preparing schemas from untrusted parties should do so in an
isolated worker or process with an operating-system memory limit. Values
checked against trusted schemas stay bounded by `stack_bytes` and, when set,
`evaluation_steps`. A prepared value contract can remain undecidable for a
particular value.

Two further cases give no verdict whatever `Limits` sets. Exact `multipleOf`
divides within a fixed arithmetic bound: when the value or the divisor has more
than 4,096 significant digits, or an exponent beyond ±20,000 once the trailing
zeros of its digits are counted in it, a division the answer needs gives no
verdict (`numeric-arithmetic-limit`); other numeric keywords compare exactly at
any size. Caller values keep unpaired UTF-16 units: keywords that compare values
exactly, such as `const`, `enum`, `required` and `uniqueItems`, decide them,
while a keyword that reads a string or member name as Unicode text, such as
`minLength`, `maxLength`, `pattern`, `properties`, `patternProperties` or
`propertyNames`, gives no verdict (`lone-surrogate-instance`) when it reaches
one.

A value contract that reaches a resource that was not supplied prepares with an
empty stand-in for it: a pass that reaches the stand-in gives no verdict
(`resource-unavailable`) naming the resource, and other values keep their
verdicts. Preparation refuses a value contract in which an `unevaluatedProperties`
or `unevaluatedItems` keyword's annotation sources reach a stand-in, since the
library reads those sources rather than evaluating them, and a reference to a
fragment of a missing resource or one that identifies no schema.
As conservative preparation, it also refuses a value contract that reaches a pattern
invalid as an ECMA-262 regular expression with Unicode semantics
(`schema-pattern-compilation`), or a schema that could apply again without
consuming the instance, such as one whose `$ref` names itself (`in-place-cycle`). An established
undefined result has its own `Undefined` outcome, separate from no-verdict,
which a custom evaluator may return; the default evaluator never does.
Unsupported capabilities and exhausted limits produce no-verdict. Handle
no-verdict in both preparation and validation.

Retain prepared value contracts for repeated validation. Paired macOS arm64 release
measurements reduced 20,000-integer evaluation from 5.61 ms to 0.39 ms and the
original twenty-pair SDK/jsonschema-rs time ratio from 12.50× to 4.84×. Sharing one
target carrier across 100 referring resources measured 8.01 ms before and 7.74 ms
after for public preparation. The case with 5,000 long references uses 50.38 MiB peak
requested heap versus 55.01 MiB before. Exact fallbacks and limits remain in
force; these are measured examples, not latency guarantees. See
[methodology and heap tradeoffs](docs/performance-boundaries.md).

Each value check chooses its detail. `validate` reports a failure without
problems; `explain` reports the library's first error, an explicit choice that
allocates in proportion to input and is made only for values within half the
nesting `stack_bytes` admits. The explained problem names the failing node by exact pointer from
the value and the failing keyword by resource and exact pointer; the value,
document or resource set finds line and column on demand. A prepared value contract
keeps only the original locations of the schemas it compiled for explanations,
never their text, its document, supplied resources or any position. Diagnostic
completeness is separate from the established value outcome.

## Discovery and kind implementations

HTTP discovery acquires an interface document with bounded decoded bytes and an
optional attempt deadline that the caller sets. Publication serves a conformant document's immutable bytes. Applications own
authentication, additional network policy and credentialed CORS. Browser Fetch
limits what redirect and network facts the SDK can observe. See
[HTTP policy](docs/http-discovery.md).

Source inspection, interface synthesis and invocation are optional Rust
companions. They define provider traits and checked callers. Kind implementations
supply source interpretation, binding behavior and acquisition policy in separate
projects. Applications choose providers; kinds remain exact opaque strings in core.
No fixed registry or universal kind-support claim follows from installing the SDK.
TypeScript does not expose inspection, synthesis or invocation APIs.

## Runtime qualifications

Rust requires 1.88. Fixed 256 KiB stack tests qualify development-toolchain code
generation, not every supported compiler or input. Older compilers may require
larger debug stacks. The [support policy](docs/support.md) records the MSRV test
exclusion and qualified hosts.

Cancellation is cooperative. A timer cannot preempt a blocking Rust poll or
synchronous Wasm call. Isolate expensive work when a hard deadline is needed.
Rust ownership and TypeScript disposal release retained objects; neither promises
an immediate reduction in allocator RSS or Wasm capacity.
