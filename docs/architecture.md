# Architecture

Normative semantics live in `openbindings`; companion behavior is imported
explicitly. Core has no transport, operation invoker, binding adapter or synthesis
layer. A private schema engine supports normative document-schema checking; the
optional default operation evaluator implements the public Rust trait boundary.

`ParsedDocument` owns an immutable exact JSON arena and lazily indexed names.
`DocumentValidation` carries evidence for every rule; only a conformant assessment
can yield `ValidatedDocument`. Typed `Document` fields represent normative
objects and preserve additional exact fields. Building and establishing conformance
are separate steps. Source views keep their owner's storage, not borrowed external
buffers; standalone document views rebase byte locations correctly.

`ValueContracts` owns a document, an explicit immutable `ResourceSet`, evaluator and
bounded preparation cache. A `ValueContract` owns required compiled state and
survives eviction or source-handle disposal. Cancellation and transient failures do
not poison healthy subsequent preparation. Resources with the same URI in distinct
contexts never share an implicit registry. The schema-space compiler builds a
private graph/projection preserving resource boundaries and dynamic scope. Only
private generated names change; error locations resolve to original sources.

`openbindings-internal-json` contains the flat source arena and exact numeric
adapter. Three renamed dependency forks retain upstream ownership and licenses;
see `DEPENDENCY-MAINTENANCE.md` for each patch and regression gate. They are not a
new supported general schema or regex engine. The default evaluator uses denying
retrievers even if a consuming Cargo graph enables dependency acquisition features.

`openbindings-http-discovery` separates portable origin/response policy,
caller-provided transport and immutable publication from its optional native HTTP
client. The TypeScript companion uses ordinary Fetch callbacks. No acquisition URL
is injected into document/schema interpretation.

`openbindings-source-inspection` is a separate optional Rust role companion. Kind
providers implement an associated-future trait; callers use a checked `Inspector`.
It reuses core authoring values while enforcing the role's narrower carrier rules.
Acquisition stays in providers. Binding fragments omit author-owned keys. It adds
no dependency from core to role behavior; its narrow Wasm probe does not extend the
published TypeScript surface. See the [role mapping](../crates/openbindings-source-inspection/README.md).

`openbindings-wasm` is a private bridge behind the first-class TypeScript facade,
separate from this new Rust companion.
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

`openbindings-interface-synthesis` is a separate optional role companion, with no
dependency on inspection. Providers return editable candidates; the `Synthesizer`
facade checks core conformance, source preservation and coverage correspondence
before delivering immutable results. Strict refusal retains the checked document
and evidence from that same call. Concrete source analysis and kind soundness
remain provider responsibilities. See the [synthesis guide](../crates/openbindings-interface-synthesis/README.md).

`openbindings-invocation` adds the optional binding-kind provider seam, document-based operation invocation, and one
checked input/output lifetime for all cardinalities. It depends on core only,
without inspection, synthesis, HTTP or a mandatory executor. Caller polling
drives bounded I/O; source interpretation and external effects remain provider
responsibilities. `operation::Invoker` adds explicit document-based selection and
per-value validation through a caller-selected evaluator, returning that same
lifecycle. Context provisioning and remote role transports remain separate work.
The invocation companion's
[public guide](../crates/openbindings-invocation/README.md) describes both entry
points and the maintained lifecycle tests.

The SDK supplies the three role contracts and checked caller APIs without
kind-specific instances. Providers own source interpretation, kind soundness and
external effects. [openbindings/openapi](https://github.com/openbindings/openapi)
is a separate project that implements these roles using the independent client.
It depends on the SDK; the SDK workspace, tools and examples do not depend on it.
