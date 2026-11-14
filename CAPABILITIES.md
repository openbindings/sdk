# Capabilities and limits

Use this SDK to read, validate, author and inspect OpenBindings interface
documents, and to evaluate caller-facing values against their input/output schemas.
Start with a working example in the [Rust guide](docs/rust-first-use.md) or
[TypeScript guide](packages/typescript/README.md).

## Documents and exact values

Core checks OpenBindings 0.2 document rules and reports met, violated or
inconclusive evidence where applicable. A parsed document keeps its exact bytes
and supports immutable edits that return a new revision; validation that establishes every rule yields a
conformant document. Unsupported specification versions
are refused separately. A document's conformance does not establish its author
claims about examples, bindings, availability or dependencies.

Exact JSON retains numeric tokens, escaped UTF-16, duplicate-member evidence,
unknown fields and original source locations. Batched edits target exact JSON
Pointers and preserve untouched bytes and opaque values. Checked conversion of ordinary application
data refuses unsupported or inexact representations. Conversion cannot restore
precision already lost by the application. Document conformance rejects an escaped
unpaired surrogate in any member name or string value as OBI-01, including opaque
content and embedded schemas. OBI-02 through OBI-14 then do not apply. Escaped
surrogate pairs and noncharacters remain permitted. Draft inspection, caller
values and supplied schema resources still retain unpaired UTF-16 units exactly,
and so do the JSON Pointers that locate their nodes.

Default parsing admits 32 MiB, 10,000 container levels and 1,000,000 nodes.
Rust `JsonValue::parse_prefix` also admits one exact value from a byte buffer,
returning its exclusive end offset or a distinct `JsonPrefixError::Incomplete`.
At EOF the host uses whole-text admission on an incomplete remainder. The host
owns separators and I/O and doubles retained input before retrying Incomplete,
unless EOF or a limit is reached first. Prefix calls inspect only the first
value and constant lookahead. The `_with_limits` form applies
the same allowances to the prefix through the value end, excluding trailing
whitespace and subsequent input. The TypeScript facade does not expose it.
`validate_document` uses the same defaults, including 10,000 container levels. These
admission limits are a setting at `validate_document_with_limits`,
`Document::parse_with_limits` and `JsonValue::parse_with_limits` (`JsonLimits`),
and at TypeScript's `validateDocument`, `Document.parse` and `parseJson`
(`limits`). `Document::new`, `Document::from_json`, `JsonValue::parse`,
`JsonValue::string` use the defaults, as do TypeScript's `Document.create()`, `ExactJson.from`,
`evaluateJson` and a prepared schema's `evaluate` and `explain` of an ordinary
value. `Publication::from_bytes` uses whole-document validation admission,
including the default 10,000-container nesting cap.
A document keeps the limits it was admitted under, and its edits apply them to
the edited document. Discovery raises only the byte limit, to its configured
body bound; a body past the default node or nesting limit comes back undetermined
and can be admitted by validating it again with larger limits through
`validate_document_with_limits`. Schema preparation
has separate footprint, cooperative SDK work and explicit SDK storage reservation
bounds through `PreparationLimits` (`max_sdk_work_units`,
`max_sdk_reservation_bytes`). Work is optional and off by default (`None`);
explicit SDK storage keeps a 512 MiB default allowance for storage held at once.
A time bound belongs to the caller through cancellation or `Some(work_units)`.
See the host-specific [250 ms suggestion](docs/performance-boundaries.md#limits-principle-november-2026).
These are not allocator-wide or CPU accounting for
dependencies and custom callbacks. Dependency construction needs separate
structural admission and measured resource qualification,
plus the selected evaluator's capability and execution limits. Parser-limit
findings use `byte-limit`, `depth-limit` or `node-limit` and describe the limit
without API advice. The [Rust guide](docs/rust-first-use.md) explains how to
choose admission limits. Ordinary Rust Serde conversion has a 128-container
ceiling. Document reports retain at most 4,096 findings and 8 MiB of pointers by
default. `ValidationLimits` and `Document::validate_with_limits` select other
retention allowances. Truncation leaves rule evidence intact. OBI-02 findings include fixed-schema
keyword, origin and optional facts, as documented in the
[API reference](docs/api-reference.md#document-finding-codes).

Reference inspection works on drafts, including missing or malformed version
declarations, under 0.2's rules and throughout the admitted depth. Unsupported
versions and duplicate names produce incomplete reports. `resource_root` names
the site's resource origin, absent exactly for the document resource. A
caller-selected retained-text allowance, 8 MiB by default, can stop inspection
with `reference-report-bytes`. Match `ReferenceReport::Complete { references }`
or `Incomplete { retained, limitation }` before consuming observations.
`ReferenceLimits` is accepted by the `_with_limits` reference methods.
See [reference inspection](docs/api-reference.md#schemas-and-value-results) for
completion, resources and accounting details.

## Schema evaluation

Core exposes an original, proof-owned `Schema` and a generic `SchemaEvaluator`
seam. Preparation borrows the schema, explicit immutable external resources,
limits and caller control. `PreparedSchema<E::Validator>` owns the resulting
validator. The seam requires neither `Send` nor `Sync` and performs no resource
acquisition. Core alone supplies no value verdict, even for a Boolean schema.
See the [evaluator integration guide](docs/schema-evaluators.md).

The optional [default evaluator](crates/openbindings-json-schema-evaluator/README.md)
retains an Original semantic plan and, for an admitted subset at default limits,
a compiled jsonschema validator. Outcome checks may use the compiled lane;
Explain always uses Original. Any non-default evaluator or preparation limit
selects Original. The [execution policy](crates/openbindings-json-schema-evaluator/README.md#execution-paths)
explains value selection, cancellation and storage accounting.

Results are `Satisfies`, `Fails`, `Undefined` and `NoVerdict`. Undefined requires
semantic dependence on a known undefined result. Missing content, unsupported
features, exhausted limits and cancellation alone do not establish it. Original
reasoning tracks validity and evaluated properties/items; unresolved recursive
components can remain conservative. Finite passing tests do not establish
unrestricted recursive completeness.

The evaluator preserves exact numeric comparisons and ECMA-262 11th-edition
Unicode-pattern semantics within its supported subset. Optional format assertion
is disabled. Standard draft-07, 2019-09 and 2020-12 external resources and supported
custom vocabularies retain their own dialect interpretation. Preparation failure
is separate from a per-value result; replacing external resources requires new
preparation. Prepared owners retain the original schema environment.

Explicit limits cover SDK work and reservations, not all dependency allocations,
CPU time or process RSS. Explanations have bounded original-location evidence;
missing detail does not weaken an established failure. See the evaluator guide
for the supported profiles and individual limits. Historical measurements and
qualification scopes are recorded in [performance boundaries](docs/performance-boundaries.md).

## Schema comparison

The optional [comparison companion](crates/openbindings-schema-comparison/README.md)
implements normalized identity, directional compatibility and interface checking
from `OB-2020-12` profile 0.1, amended in interfaces revision
`a3c19658fc89815d7a417eae29ba31fd4e930daf`. It preserves optional
slots, exact numbers/strings, opaque keyword values and union multiplicity. Explicit
caller roots, bases and resource records govern references; it performs no fetching
and does not reuse core `$id` interpretation or value evaluation. Its identity answer
is profile identity, not general schema equivalence or a verified reuse claim.

The comparison fixture harness passes 30 ordinary and 10 exact identity groups,
plus all 62 ordinary and 29 exact directional groups (including expected schema
errors), with no skipped groups. Directional verdicts distinguish compatible,
incompatible and indeterminate; interface checking retains every operation,
every distinct alias match, realization/consumption selected by a callback on
each required/provided pair (or a missing match), skipped slots
and comparison failures. Missing operations have one correspondence with no role;
present correspondences expose one role through `Option<OperationRole>`.
Cancellation returns an error; exhausted allowances remain slot failures.
Callers choose summary treatment of failures;
`Verdict::collapse` takes verdicts only. Reusable `ComparisonContext`s index
documents lazily and keep completed indexing after successful work. Both-union
positions fail closed when non-identical. Retained applicator references compare
normalized targets, and `$defs` retention is local to each schema subtree. Reference-rule identity lets pullers
keep same-spelled accounted references as leaves and ask whether remaining drift
lies only behind unaccounted targets. The caller supplies accounting.
Missing/cyclic followed references, repeated-first-enum intersection ambiguity,
resource exhaustion and cancellation are failures with no verdict. Defaults are
no work cap (`max_work: None`), 64 MiB conservative reservations for storage held
at once and depth 128. The caller chooses a time bound through cancellation or
`Some(work_units)`. Scratch drops after each slot; shared indexes, normalized
targets and retained reports stay charged. The depth
ceiling cannot be raised. Plain-name anchors and nested resource bases are supported.
Comparison remains independent of core conformance.

## Discovery and kind implementations

HTTP discovery acquires an interface document with bounded decoded bytes and an
optional attempt deadline that the caller sets. Publication serves a conformant document's immutable bytes. Applications own
authentication, additional network policy and credentialed CORS. Browser Fetch
limits what redirect and network facts the SDK can observe. See
[HTTP policy](docs/http-discovery.md).

Invocation and [kind description/source checking](crates/openbindings-kind-description/README.md)
are optional Rust companions. They define provider traits and checked callers.
Kind descriptions retain per-operation support claims and disclose check, pull and invoke execution;
source checks require an explicit call and never acquire context or retry by
themselves. Shared Context admission refuses repeated decoded member names at
every depth, including objects inside arrays. This is SDK policy, since the
context document is silent on duplicates; it follows OBI-01's parser-differential
reason. Exact carriers retain allowed output members, numbers, strings and
absent-versus-null data. Neither a description nor a checked source proves document
conformance, availability or permission to execute. Default carrier handoff bounds
are 32 MiB per request/response and 100,000 listed kinds; they do not bound work
inside a provider. Native provider failures remain distinct from kind rejections.

Kind implementations supply source interpretation, binding behavior and acquisition
policy in separate projects. Applications choose providers; kinds remain exact
opaque strings in core. No fixed registry or universal kind-support claim follows
from installing the SDK. TypeScript does not expose interface APIs.

[Source pulling](crates/openbindings-source-pull/README.md) provides exact carriers,
original-document requests, borrowed providers and checked lazy callers. The SDK
checks mechanically knowable report relationships and core conformance of a
prospective all-additions snapshot. It does not establish kind-owned fidelity,
offered-schema identity or opaque dependency closure, and it never applies
proposals. Default handoffs are 32 MiB; relationship indexing admits 100,000
entries/uses labels and 64 MiB of conservative reservations for storage held at
once. Relationship work is uncapped (`max_relationship_work: None`) by default;
the caller chooses a time bound through cancellation or `Some(work_units)`.
Per-check uses scratch and completed relationship indexes are released. These do not cover provider work or core prospective
rebuild/validation internals, which retain core limits. Cancellation is checked
around those synchronous core calls. These checks do not qualify an application
or adapter for deployment.

## Runtime qualifications

Rust requires 1.88. Fixed 256 KiB stack tests qualify development-toolchain code
generation, not every supported compiler or input. Older compilers may require
larger debug stacks. The [support policy](docs/support.md) records the MSRV test
exclusion and qualified hosts.

Cancellation is cooperative. A timer cannot preempt a blocking Rust poll or
synchronous Wasm call. Isolate expensive work when a hard deadline is needed.
Rust ownership and TypeScript disposal release retained objects; neither promises
an immediate reduction in allocator RSS or Wasm capacity.
