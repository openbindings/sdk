# Maintainer architecture

Normative semantics live in `openbindings`; companion behavior is imported
explicitly. Core has no transport, operation invoker, binding adapter or synthesis
layer. A private schema engine supports normative document-schema checking; the
optional default operation evaluator implements the public Rust trait boundary.

`Document` owns an immutable exact JSON arena and lazily indexed names.
`DocumentValidation` carries evidence for every rule; only a conformant validation
can yield `ConformantDocument`. `Document` edits return a new immutable snapshot through atomic exact-text batches and
typed part setters. Editing commits a new arena and fresh caches; clones and proofs
retain the earlier revision. `ConformantDocument` exposes no mutable access. Source views keep their owner's storage, not borrowed external
buffers; standalone document views rebase byte locations correctly.

A conformant operation hands out its `Schema`s; preparation borrows an
evaluator and an explicit immutable `ExternalSchemas`. Each preparation compiles
independently. A caller-retained `PreparedSchema` owns required compiled state
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

`openbindings-context` owns opaque context objects and the current closed
`{message, schema}` challenge carrier. It depends only on core exact JSON and
performs no acquisition, schema evaluation, trust decision or retry. Invocation,
kind checking and pulling consume this shared interface concept; it adds no OBI
fields. Each role consumes this contract without adding a dependency to core.

`openbindings-kind-description` separates description, check-description and actual
source checking. Support claims are per operation. Its finite clients retain
native provider failures, exact carriers and the shared context challenge.
Queries never trigger a source check or context acquisition.

`openbindings-source-pull` owns pull additions and reports, original-document
requests, borrowed providers and a checked `PullClient`. Providers project from
the selected source according to its kind. The client checks report relationships
and core conformance of the prospective additions; the caller explicitly chooses
what to adopt. Proposals never edit the original document automatically.
[Source pulling](../crates/openbindings-source-pull/README.md) documents these
responsibilities and the limits of mechanical validation.

`openbindings-schema-comparison` supplies normalized schema identity for authoring
jobs such as operation reuse. Each side retains its original reference context.
Identity differs from general equivalence or directional compatibility, and a
comparison failure is not a negative answer. Existing-entry drift uses the kind's
would-write comparison, which may include annotations that identity ignores.

`openbindings-invocation` supplies `BindingInvoker`, `BindingClient` and
`OperationClient`. Both clients retain the original conformant document and
selected authored keys. Operation invocation adds explicit selection and per-value
checks using the caller's evaluator and external schemas. Bounded streaming I/O
is driven by caller polling; binding execution and effects belong to the provider.
Preflight is a separate advisory call. Context acquisition and retries remain
application decisions. See the [invocation guide](../crates/openbindings-invocation/README.md).

Async companions share the cancellation identity and scoped delivery helper in
`openbindings-internal-cancellation`. Shared opaque context and challenges belong
to `openbindings-context`; there is no universal supported-kind verdict or detached
by-value binding interface. Core depends on none of these role companions. The
finite clients and streaming invocation driver have distinct lifecycles, and no
particular runtime or blanket Send/Sync requirement is imposed on providers.

The SDK supplies contracts and checked caller APIs without kind-specific
instances. Providers own source interpretation, kind soundness and effects.
[openbindings/openapi](https://github.com/openbindings/openapi) is a separate
implementation project using an independent client. The SDK workspace, tools and
examples do not depend on it.
