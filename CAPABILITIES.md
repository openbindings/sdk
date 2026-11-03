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
content and embedded schemas. OBI-02 through OBI-14 then do not apply. Escaped
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

## Schema evaluation

Core exposes an original, proof-owned `Schema` and a generic `SchemaEvaluator`
seam. Preparation borrows the schema, explicit immutable external resources,
limits and caller control. `PreparedSchema<E::Validator>` owns the resulting
validator. The seam requires neither `Send` nor `Sync` and performs no resource
acquisition. Core alone supplies no value verdict, even for a Boolean schema.
See the [evaluator integration guide](docs/schema-evaluators.md).

The optional [default evaluator](crates/openbindings-json-schema-evaluator/README.md)
combines its exact primitive/compiled engine with a bounded semantic interpreter
over original schema locations. It distinguishes `Satisfies`, `Fails`, `Undefined`
and `NoVerdict`. A known invalid Unicode pattern or a supported non-consuming
alias cycle may establish Undefined when the value's validity depends on it.
Missing content, an unsupported predicate, cancellation and an exhausted limit
are not known undefined effects. Decisive surrounding logic may establish a
result despite an effect elsewhere.

Repeated original predicates share exact value identity. Applicator reasoning
tracks evaluated properties and items as well as validity; effects do not invent
annotations. Unknown external content may correlate with addressable predicates.
The anonymous document resource has a distinct privacy boundary. Recursive
summaries are reusable only when validity and every evaluated annotation are
independent of provisional cuts; unresolved components remain conservative.
Finite successful tests are not a claim of unrestricted recursive completeness.

The compiled path uses jsonschema 0.58.6 with exact decimal comparisons and the
SDK's ECMA-262 11th-edition Unicode-pattern handling. Unsupported valid pattern
features yield NoVerdict where required; invalid patterns participate in semantic
dependence analysis. Optional `format` assertion remains disabled. External
draft-07 and 2019-09 resources now use their own inventory, assertions and
annotations in the semantic path. Draft-07 reference siblings are ignored;
2019-09 recursive references and contains annotations retain their dialect's
behavior. Unsupported identifier forms retain explicit capability refusals,
including external identity selection when a stopped inventory could hide names.
Required custom vocabulary and format-assertion interpretation remains unfinished.

Construction and evaluation honor explicit resource/work limits and cooperative
cancellation. The semantic path charges its decision graph, annotations, value
identity and proof work, including retries. Original diagnostics are bounded
before copying long paths; supported pattern matching borrows input text.
`program_bytes` does not promise a total process-RSS cap or account for every
allocation in the underlying compiler. Exact `multipleOf` retains its arithmetic
bound; some Unicode-text operations cannot interpret unpaired UTF-16 units.
See the evaluator guide's limit table for the individual policies.

Preparation failure is separate from a per-value outcome. A failed or cancelled
attempt does not poison cached preparation. Complete compiled validators release
unnecessary original sources; a semantic validator retains the original schema
environment it needs. `evaluate` requests an outcome; `explain` may provide bounded
original-location evidence. Missing diagnostic detail never changes the verdict.

Current migration gates are not all green: the native strict core has 505/505
exact answers, and the native required evaluator kit has 1537/1546; the nine
remaining cases require custom vocabulary interpretation. The last complete
auxiliary host replay passed 2348 preserved/adjudicated inputs; full Wasm/host
qualification must be refreshed after the standard-dialect changes. Independently reviewed recursive controls close
three earlier F gaps, but two further required Undefined cases remain open.
The migration's matched-toolchain performance and all-host qualification must be
completed before claiming these changes fully qualified. Earlier measurements in
[performance boundaries](docs/performance-boundaries.md) remain historical evidence.

## Schema comparison

The optional [comparison companion](crates/openbindings-schema-comparison/README.md)
implements normalized identity from `OB-2020-12` profile 0.1. It preserves optional
slots, exact numbers/strings, opaque keyword values and union multiplicity. Explicit
caller roots, bases and resource records govern references; it performs no fetching
and does not reuse core `$id` interpretation or value evaluation. Its Equal answer
is profile identity, not general schema equivalence or a verified reuse claim.

All 14 ordinary and 10 exact identity fixture groups pass. The 88 directional
groups remain outside this API's scope. Missing/cyclic references, unsupported
plain-name anchors, repeated-first-enum intersection ambiguity, resource exhaustion
and cancellation are failures with no identity verdict. Defaults are 20 million
work units, 64 MiB cumulative allocation reservations and depth 128; the depth
ceiling cannot be raised. Comparison remains independent of core conformance.

## Discovery and kind implementations

HTTP discovery acquires an interface document with bounded decoded bytes and an
optional attempt deadline that the caller sets. Publication serves a conformant document's immutable bytes. Applications own
authentication, additional network policy and credentialed CORS. Browser Fetch
limits what redirect and network facts the SDK can observe. See
[HTTP policy](docs/http-discovery.md).

Invocation and [kind description/source checking](crates/openbindings-kind-description/README.md)
are optional Rust companions. They define provider traits and checked callers.
Kind descriptions retain per-operation support claims and disclose check execution;
source checks require an explicit call and never acquire context or retry by
themselves. Exact carriers retain allowed output members, numbers, strings and
absent-versus-null data. Neither a description nor a checked source proves document
conformance, availability or permission to execute. Default carrier handoff bounds
are 32 MiB per request/response and 100,000 listed kinds; they do not bound work
inside a provider. Native provider failures remain distinct from kind rejections.

Kind implementations supply source interpretation, binding behavior and acquisition
policy in separate projects. Applications choose providers; kinds remain exact
opaque strings in core. No fixed registry or universal kind-support claim follows
from installing the SDK. TypeScript does not expose role APIs.

[Source pulling](crates/openbindings-source-pull/README.md) provides exact carriers,
original-document requests, borrowed providers and checked lazy callers. The client
checks mechanically knowable report relationships and core conformance of a
prospective all-additions snapshot. It does not establish kind-owned fidelity,
offered-contract identity or opaque dependency closure, and it never applies
proposals. Default handoffs are 32 MiB; relationship indexing has separate entry,
work and allocation bounds. These do not cover provider work or core prospective
rebuild/validation internals, which retain core limits. Cancellation is checked
around those synchronous core calls. Independent review and legacy consumer
replacement remain in progress; obsolete inspection/synthesis companions are not
current-interface claims.

## Runtime qualifications

Rust requires 1.88. Fixed 256 KiB stack tests qualify development-toolchain code
generation, not every supported compiler or input. Older compilers may require
larger debug stacks. The [support policy](docs/support.md) records the MSRV test
exclusion and qualified hosts.

Cancellation is cooperative. A timer cannot preempt a blocking Rust poll or
synchronous Wasm call. Isolate expensive work when a hard deadline is needed.
Rust ownership and TypeScript disposal release retained objects; neither promises
an immediate reduction in allocator RSS or Wasm capacity.
