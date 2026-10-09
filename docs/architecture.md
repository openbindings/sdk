# Architecture

Normative semantics live in `openbindings`; companion behavior is imported
explicitly. Core has no transport, operation invoker, binding adapter or synthesis
layer. A private schema engine supports normative document-schema checking; the
optional default operation evaluator implements the public Rust trait boundary.

`ParsedDocument` owns an immutable exact JSON arena and lazily indexed names.
`DocumentAssessment` carries evidence for every rule; only a conformant assessment
can yield `ValidatedDocument`. Typed `DocumentBuilder` fields represent normative
objects and preserve additional exact fields. Building and establishing conformance
are separate steps. Source views keep their owner's storage, not borrowed external
buffers; standalone document views rebase byte locations correctly.

`ValueContracts` owns a document, an explicit immutable `ResourceSet`, evaluator and
bounded preparation cache. A `PreparedContract` owns required compiled state and
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

`openbindings-wasm` is a private bridge behind the first-class TypeScript facade.
Rust owns semantic work and exact values. The facade exposes discriminated outcomes,
checked JS conveniences, immutable resource sets and disposable retained handles.
Repeated UI metadata reads use cached JS snapshots. It performs no second schema
implementation. Initialization is explicit and asynchronous; CPU calls are
synchronous. Workers are the interruptible bulk-work route. workerd initialization
occurs in an active request so host entropy is available, then is reused.

The six public Rust workflows and TypeScript tests are architecture witnesses,
not sample applications to be migrated. Broad invoker/synthesizer APIs, arbitrary
JavaScript evaluator callbacks, a Node native addon, source retrieval and CLI/Panjir
migration remain separate design work.
