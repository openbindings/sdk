# SDK architecture

The SDK follows the pinned OpenBindings document model and the independently
published interfaces. Core defines document semantics. Optional companions define
schema evaluation, discovery, comparison and interface roles. Core depends on none
of those companions, and no source kind becomes part of core.

## Documents and original schemas

`Document` owns an immutable exact JSON snapshot, including editor drafts that do
not conform. `DocumentValidation` reports evidence for each document rule; only
established conformance produces a `ConformantDocument`. Its canonical `Operation`,
`Source`, `Binding`, `Dependency` and `Example` entries retain their originating
revision. Exact JSON preserves absent fields, null, numeric tokens and UTF-16 string
meaning. Typed domain entries are acquired from a conformant document.

Authoring uses explicit `set`, `remove` or atomic `edit` operations to produce a new
snapshot. Untouched bytes remain exact; edits do not fabricate missing containers.
Existing entries, proofs and schema owners keep their original revision. There are
no parallel detached authoring-part types or typed setter families.

An operation's `input()` and `output()` expose contextual `Schema` owners when a
schema is present. Named schemas come from `ConformantDocument::schema`.
`EvaluationRequest` borrows the selected original schema and explicit immutable
`ExternalSchemas`. Its owned `SchemaEnvironment` supplies original locations,
resource/profile inventories and typed reference queries. It does not fetch
missing content, rewrite the document or introduce a global resource registry.
Cached environment construction is scoped to the actual document/resource pair;
changing either cannot alter an existing prepared owner's meaning.

`SchemaEvaluator` prepares an owned `SchemaValidator`; `PreparedSchema` supplies
the caller-facing wrapper. Prepared owners survive their input owners. They may
retain immutable originals or prepared facts needed for evaluation and diagnostics;
there is no requirement that every implementation release original arenas during
preparation. Preparation failure is separate from each value's satisfies, fails,
undefined or no-verdict outcome. `Detail::Explain` requests bounded diagnostics.

`openbindings-internal-json` implements exact storage and numeric operations.
`openbindings-internal-schema` supports fixed normative document checks and original
schema interpretation. These private implementation packages do not create new
public document concepts. Fixed document checks are available without an optional
value evaluator.

## Default evaluation implementation

The optional `openbindings-json-schema-evaluator` uses one prepared semantic plan
over original schema locations. It
keeps exact values, typed reference effects, conditional evaluation annotations
and supported recursive proofs together. Prepared original queries and pattern
readings are immutable; mutable matching and proof state belongs to each call.
No completed artifact retains a caller's preparation control.

The outgoing projection compiler and its adaptation code have been removed.
Pattern construction uses checked structural admission, a defined retained
footprint and cooperative SDK work/storage budgets. These policies do not promise
full accounting of dependency allocations or total process memory.
See the [saved checkpoint](alignment-checkpoint.md) for actual completed increments
and remaining gates. Default workload and performance qualification remain required.
The SDK uses unmodified dependencies; maintenance details are
in [DEPENDENCY-MAINTENANCE.md](../DEPENDENCY-MAINTENANCE.md).

## Current interfaces

`openbindings-context` owns opaque context objects and the closed `{message, schema}`
challenge carrier. It performs no acquisition, schema evaluation, trust decision
or retry, and adds no OBI document fields.

`openbindings-kind-description` separates listing and description from explicit
source checking. Support claims are per operation; check descriptions disclose
whether the implementation executes operations. Checked callers preserve exact
carriers and distinguish rejection, context challenge and operational failure.
Describing a kind never triggers a source check.

`openbindings-source-pull` owns exact additions/reports, original-document requests,
borrowed providers and `PullClient`. A provider projects the selected source
according to its kind. The client checks mechanical relationships and prospective
document conformance. The caller chooses and applies changes; proposals remain
unapplied. Generic checks cannot certify a provider's fidelity or opaque data's
complete dependencies.

`openbindings-schema-comparison` implements the adopted normalized identity profile
with an original reference context on each side. Identity is separate from general
equivalence and directional compatibility. A comparison failure is not a negative
answer. Existing-binding drift uses the kind's would-write comparison, which may
include annotations the identity profile ignores.

`openbindings-invocation` provides `BindingInvoker`, `BindingClient` and
`OperationClient`. Checked invocation retains the original document and selected
keys. Operation invocation adds selection and per-value schema gates through the
caller's evaluator and resources. Caller polling drives bounded streaming I/O;
providers own realization effects. Preflight is separate and advisory. Context
acquisition, permission and retries remain application decisions.

Async companions share scoped cancellation delivery through
`openbindings-internal-cancellation`. Finite calls and streaming invocation retain
their distinct lifecycles. There is no required runtime, blanket Send/Sync provider
bound, universal supported-kind verdict or detached binding carrier. Obsolete
inspection/synthesis interfaces are not preserved as compatibility layers.

## Hosts and discovery

`openbindings-http-discovery` separates portable origin/response policy,
caller-provided transport and immutable publication from its optional native HTTP
client. The TypeScript companion uses Fetch callbacks. Acquisition does not inject
a new base into schema interpretation.

`openbindings-wasm` is the private bridge for the TypeScript facade. Rust owns
semantic work and exact values; TypeScript supplies language-appropriate tagged
outcomes, checked conveniences and disposable owners. Its implementation modules
follow JSON, documents, schemas, diagnostics, ownership, runtime and optional HTTP
discovery. There is no second JavaScript schema engine or compatibility barrel.

Initialization is explicit and asynchronous; CPU calls are synchronous. Workers
provide interruption for bulk work. workerd initialization occurs in an active
request and can then be reused. Rust interface companions are not exposed as
TypeScript role callbacks. External source/archive consumers and actual host
probes qualify their stated scope; fixture bridges are not new public SDK APIs.

Kind-specific adapters, protocol clients and applications remain independent.
They supply source interpretation, kind soundness and effects. SDK qualification
does not establish their conformance or deployment readiness.
