# Capabilities and limits

Use this SDK to read, validate, author and inspect OpenBindings interface
documents, and to validate caller-facing values against their value contracts.
Start with a working example in the [Rust guide](docs/rust-first-use.md) or
[TypeScript guide](packages/typescript/README.md).

## Documents and exact values

Core checks OpenBindings 0.2 document rules and reports met, violated or
inconclusive evidence where applicable. Parsing preserves an editor snapshot;
conformance establishes a validated document. Unsupported specification versions
are refused separately. A document's conformance does not establish its author
claims about examples, bindings, availability or dependencies.

Exact JSON retains numeric tokens, escaped UTF-16, duplicate-member evidence,
unknown fields and original source locations. Typed authoring preserves opaque
values while re-encoding typed fields. Checked conversion of ordinary application
data refuses unsupported or inexact representations. Conversion cannot restore
precision already lost by the application.

Default parsing admits 64 MiB, 10,000 container levels and 1,000,000 nodes.
Ordinary Rust Serde conversion has a 128-container ceiling. Document reports
retain at most 4,096 findings and 8 MiB of pointers. Truncation leaves rule evidence
intact. See [performance and allocation boundaries](docs/performance-boundaries.md).

## Value validation

Document validation needs no caller-supplied evaluator. Operation values require
an explicit Rust `SchemaEvaluator`; the TypeScript facade supplies the bundled
evaluator. Core alone makes no satisfies/fails decision, even for a boolean
schema. The [evaluator guide](docs/schema-evaluators.md) explains integration.

The optional default evaluator supports JSON Schema 2020-12 with exact decimal
comparison and explicit immutable resources. It performs no reference acquisition.
`format` remains an annotation. Applied Unicode property matching and other
unsupported capabilities produce no-verdict. Work and regex budgets can also
prevent a verdict; a ready contract can remain undecidable for a particular value.

Missing content or undefined reference results do not prevent a verdict when the
evaluator proves that they cannot affect it. Semantic undefinedness, missing
resources and implementation inability retain distinct reasons. Applications must
handle no-verdict in both preparation and validation.

Coarse failure diagnostics are the default. Upstream diagnostics are an explicit
choice with library allocation and stack limits. They can allocate in proportion
to input and overflow on deep values. Diagnostic completeness is separate from
the established value outcome.

## Discovery and kind implementations

HTTP discovery acquires an interface document with bounded decoded bytes and an
attempt deadline. Publication serves validated immutable bytes. Applications own
authentication, additional network policy and credentialed CORS. Browser Fetch
limits what redirect and network facts the SDK can observe. See
[HTTP policy](docs/http-discovery.md).

Source inspection, interface synthesis and invocation are optional Rust
companions. They define provider traits and checked callers. Kind implementations
supply source interpretation, binding behavior and acquisition policy in separate
projects. Applications choose providers; kinds remain exact opaque strings in core.
No fixed registry or universal kind-support claim follows from installing the SDK.
Public TypeScript role APIs are not provided.

## Runtime qualifications

Rust requires 1.88. Fixed 256 KiB stack tests qualify development-toolchain code
generation, not every supported compiler or input. Older compilers may require
larger debug stacks. The [support policy](docs/support.md) records the MSRV test
exclusion and qualified hosts.

Cancellation is cooperative. A timer cannot preempt a blocking Rust poll or
synchronous Wasm call. Isolate expensive work when a hard deadline is needed.
Rust ownership and TypeScript disposal release retained objects; neither promises
an immediate reduction in allocator RSS or Wasm capacity.
