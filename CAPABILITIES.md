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

Default parsing admits 64 MiB, 10,000 container levels and 1,000,000 nodes.
Ordinary Rust Serde conversion has a 128-container ceiling. Document reports
retain at most 4,096 findings and 8 MiB of pointers. Truncation leaves rule evidence
intact. See [performance and allocation boundaries](docs/performance-boundaries.md).
OBI-02's whole-document check may spend 80 evaluator steps per admitted node and
64 pattern steps per source byte, so every document the admission limits admit,
default or caller-selected, gets an OBI-02 verdict, with work linear in those
limits. A near miss of the SemVer pattern is the costliest input, at about 20
pattern steps per byte, which took about 0.37 µs per byte in a release build on
an Apple M1 Max: a 60 MB near miss takes about 22 seconds to reach its verdict,
and one at the default 64 MiB limit about 25 seconds. The admission byte limit is
how a caller bounds that time: `validate_document_with_limits` with a smaller
`max_bytes` admits less. Each embedded schema's OBI-10 meta-schema check grows the
same way with that schema, 129 steps and 64 pattern steps per source byte,
so every schema the admission limits admit gets a meta-schema verdict. A check
reads its schema's own members and only the type of each subschema, so a node is
read by at most two checks and their work together stays linear in those limits.

## Value validation

Document validation needs no caller-supplied evaluator. Operation values require
an explicit Rust `SchemaEvaluator`; the TypeScript facade supplies the bundled
evaluator. Only a conformant operation hands out its input and output value
contracts, and preparing one borrows the evaluator and any supplied resources. Core alone makes no satisfies/fails decision, even for a boolean
schema. The [evaluator guide](docs/schema-evaluators.md) explains integration.

The optional default evaluator supports JSON Schema 2020-12 with exact decimal
comparison and explicit immutable resources. It performs no reference acquisition.
`format` remains an annotation. Applied Unicode property matching and other
unsupported capabilities produce no-verdict. The default work allowance grows
with the value checked: 2,000,000 units per pass up to 65,536 value nodes and
proportionally more above, so ordinary schemas decide values as large as the
default admission limits accept. An explicitly configured allowance caps every
pass at that many units, whatever the value's size. The evaluation depth (1,024 nested
evaluations, a stack bound) and the regex budget (2,000,000 steps, the safety
bound) can still prevent a verdict; a prepared contract can remain undecidable
for a particular value. Patterns may nest 32 groups by default, which keeps
preparation within a 256 KiB thread stack.

Missing content or undefined reference results do not prevent a verdict when the
evaluator proves that they cannot affect it. An established undefined result has
its own `Undefined` outcome, separate from no-verdict. The default evaluator
establishes it for static reference defects with original-source provenance and
two admissible completions that disagree. Other undefined results remain outside
its supported fragment. Missing resources, unsupported capabilities and exhausted
budgets produce no-verdict. Handle no-verdict in both preparation and validation.

Retain prepared contracts for repeated validation. Paired macOS arm64 release
measurements reduced 20,000-integer evaluation from 5.61 ms to 0.39 ms and the
original twenty-pair SDK/jsonschema-rs time ratio from 12.50× to 4.84×. Sharing one
target carrier across 100 referring resources measured 8.01 ms before and 7.74 ms
after for public preparation. The case with 5,000 long references uses 50.38 MiB peak
requested heap versus 55.01 MiB before. Exact fallbacks and budgets remain in
force; these are measured examples, not latency guarantees. See
[methodology and heap tradeoffs](docs/performance-boundaries.md).

Each value check chooses its detail. `validate` reports a failure without
problems; `explain` reports the library's first error, an explicit choice with
library allocation and stack limits. It can allocate in proportion to input and
overflow on deep values. Each explained problem names the failing node by exact
pointer from the value and the failing keyword by resource and exact pointer; the
value, document or resource set finds line and column on demand. A prepared
contract keeps a compact copy of the keywords it compiled for explanations, never
its document, supplied resources or any position. Diagnostic completeness is
separate from the established value outcome.

## Discovery and kind implementations

HTTP discovery acquires an interface document with bounded decoded bytes and an
attempt deadline. Publication serves a conformant document's immutable bytes. Applications own
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
