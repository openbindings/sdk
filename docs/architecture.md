# Maintainer architecture

Normative semantics live in `openbindings`; companion behavior is imported
explicitly. Core has no transport, operation invoker, binding adapter or synthesis
layer. A private schema engine supports normative document-schema checking; the
optional default operation evaluator implements the public Rust trait boundary.

`Document` owns an immutable exact JSON arena and lazily indexed names.
`DocumentValidation` carries evidence for every rule; only a conformant validation
can yield `Conformant<Document>`. The same `Document` is edited in place through atomic exact-text batches and
typed part setters. Editing commits a new arena and fresh caches; clones and proofs
retain the earlier revision. `Conformant<Document>` exposes no mutable access. Source views keep their owner's storage, not borrowed external
buffers; standalone document views rebase byte locations correctly.

A conformant operation hands out its `ValueContract`s; preparation borrows an
evaluator and an explicit immutable `ResourceSet`. Each preparation compiles
independently. A caller-retained `PreparedValueContract` owns required compiled state
and survives document, resource and evaluator disposal. Cancellation and transient failures do
not poison healthy subsequent preparation. Resources with the same URI in distinct
sets never share an implicit registry. A set indexes its resource identities once
for the core projection.
A resolution space is assembled only when an evaluator requests the core
projection, once per live document revision and resource set: the space without
supplied resources stays with the revision, and a space with a set stays with
that set until either the set or the revision is dropped. The default evaluator
assembles its own space for each preparation and drops it afterwards; the space
shares the set's identity index and schema positions and the revision's schema
index instead of building them again, and takes the revision's schemas as
already checked against the 2020-12 meta-schemas once its validation has checked
each of them through that index and found none failing.
Core gives the default evaluator its projection as definitions rather than as
JSON text to parse again; it writes the text only when the projection could
reach the text's fixed bounds, which then decide as they do for the text. The
evaluator reads each `pattern` from the source the definitions borrow rather
than from a copy: the library meets only its matcher's index. Every space
places the packaged 2020-12 meta-schemas after the document and supplied resources
as one immutable layer, built once per process and shared, so assembly does not
re-add them. The schema-space compiler builds a
private graph/projection preserving resource boundaries and dynamic scope. Only
private generated names change; error locations resolve to original sources.

`openbindings-internal-json` contains the flat source arena and exact numeric
arithmetic. Core's fixed checks and reference inspection and the default
evaluator all use the unmodified jsonschema library; the SDK carries no fork of
any dependency. The evaluator bundles each projected value contract into one resource,
resolves dynamic references itself, reads patterns for the regex crate and
applies one schema transform that keeps the library's `unevaluatedProperties`
walk from a known bug; see `DEPENDENCY-MAINTENANCE.md` for the composition, the
separation and the regression gates. Core and the default evaluator use denying
or local retrievers even if a consuming Cargo graph enables dependency
acquisition features.

`openbindings-http-discovery` separates portable origin/response policy,
caller-provided transport and immutable publication from its optional native HTTP
client. The TypeScript companion uses ordinary Fetch callbacks. No acquisition URL
is injected into document/schema interpretation.

`openbindings-source-inspection` is a separate optional Rust interface companion. Kind
providers implement an associated-future trait; callers use a checked `Inspector`.
It reuses core authoring values while enforcing the interface's narrower carrier rules.
Acquisition stays in providers. A target's binding is the by-value `BindingValue`
invocation also takes, without author-owned keys. It adds
no dependency from core to interface behavior; its narrow Wasm probe does not extend the
published TypeScript surface. See the [interface mapping](../crates/openbindings-source-inspection/README.md).

`openbindings-wasm` is a private bridge behind the first-class TypeScript facade,
separate from the Rust interface companions.
Rust owns semantic work and exact values. The facade exposes discriminated outcomes,
checked JS conveniences, immutable resource sets and disposable retained handles.
Repeated UI metadata reads use cached JS snapshots. It performs no second schema
implementation. Initialization is explicit and asynchronous; CPU calls are
synchronous. Workers are the interruptible bulk-work route. workerd initialization
occurs in an active request so host entropy is available, then is reused.

The six public Rust workflows and TypeScript tests are architecture witnesses,
not sample applications to be migrated. Arbitrary
JavaScript evaluator callbacks, a Node native addon, source retrieval and CLI/Panjir
migration remain separate design work.

`openbindings-interface-synthesis` is a separate optional interface companion, with no
dependency on inspection. Providers return editable candidates; the `Synthesizer`
facade checks core conformance, source preservation and coverage correspondence
before delivering immutable results. Strict refusal retains the checked document
and evidence from that same call. Concrete source analysis and kind soundness
remain provider responsibilities. See the [synthesis guide](../crates/openbindings-interface-synthesis/README.md).

`openbindings-invocation` adds the optional binding-kind provider seam, document-based operation invocation, and one
checked input/output lifetime for all cardinalities. It depends on core only,
without inspection, synthesis, HTTP or a mandatory runtime. Caller polling
drives bounded I/O; source interpretation and external effects remain provider
responsibilities. `operation::Invoker` adds explicit document-based selection and
per-value validation through a caller-selected evaluator, returning that same
lifecycle. Context provisioning and remote interface transports remain separate work.
The invocation companion's
[public guide](../crates/openbindings-invocation/README.md) describes both entry
points and the maintained lifecycle tests.

The three kind-facing interfaces define `SupportedKind` and `KindSupportVerdict`
identically, and the source inspector and binding invoker share one by-value
`Binding` schema. The SDK defines these records once, in the internal
`openbindings-internal-interfaces` crate, with the by-value `BindingValue`'s
structural `BoundaryError` and carrier readers; inspection, synthesis and
invocation reexport the records as all four async companions reexport
cancellation. The carrier readers stay hidden delivery internals, and they mint a
`BoundaryError` only from a closed set of codes, so no dependent can attach an
arbitrary code to one. Inspection's error converts from it. A
provider serving several jobs builds one advisory list, and an inspected target's
binding is the binding an invoker receives. Core stays free of interface records.
Each companion still refuses an empty requested kind as `empty-kind` in its own
code namespace and keeps a bounded JSON writer mapped to its own error type.
`BindingValue` reads `preference` with its own exact parser, since core's document
reader is private. Tests hold the remaining copies aligned: the published
kind-support cases run through all three facades, each facade's empty-kind code
is pinned, the writers share byte-limit tests, and a differential test compares
the preference parser with core's document reading. Change the copies together.

The SDK supplies the three interface contracts and checked caller APIs without
kind-specific instances. Providers own source interpretation, kind soundness and
external effects. [openbindings/openapi](https://github.com/openbindings/openapi)
is a separate project that implements these interfaces using the independent client.
It depends on the SDK; the SDK workspace, tools and examples do not depend on it.
