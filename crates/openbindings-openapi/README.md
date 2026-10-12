# OpenAPI adapter

Optional OpenBindings integration using the independently maintained dynamic
OpenAPI client. The normative SDK core does not depend on this crate.

This first, unpublished slice provides source inspection for the candidate
`openbindings.openapi-3.1@1` (3.1.0–3.1.2). Use `Inspector::new(OpenApi::default())`
and `inspect(&source).await`. The provider accepts an object or JSON/YAML string
in `content.document`, with an optional real retrieval URI in `content.location`.
It never retrieves bytes or invents a reference base. Location-only input without
entry bytes is an unavailable-source refusal. Caller-supplied reference resources
are configured through `OpenApi::new(Options)` using the client's `ResourceInput`.
A new provider establishes a new immutable resource/limit configuration.

The result contains mounted target pointers and optional operation-key suggestions.
Authors choose their own OBI keys. No operation schemas, idempotency guarantees,
synthesis or invocation support are implied. A target identifies an operation;
it does not certify that every possible request can execute. Subordinate malformed
parameters, servers or response declarations are work for subsequent capabilities.

`exhaustive` refers to the admitted target inventory. Missing Path Item resources,
required resource layouts, or client analysis limitations yield a partial inventory
with diagnostic evidence. Missing bytes never silently trigger network access.
Malformed primitive method values are not Operation Objects. Empty inventories
are valid. Duplicate operation IDs preserve both distinct mounted targets; this
adapter never selects a winner by ID. The SDK's stable output and duplicate
sourceRef checks are implementation policies, not new published role requirements.

Unknown Path Item members remain inert. When a standard method appears in
multiple reference layers, that method is not a target; unaffected methods remain
available. The client supplies the resolved chain and typed conflict evidence.
Missing dependencies and unresolved shared-field conflicts still produce partial
analysis. Explicit layouts for untyped bundles and URI userinfo admission remain
client limitations. This package is **not a complete OpenAPI 3.1 implementation**;
synthesis and invocation are not implemented, and other version kinds are declined.

One private prepared entry retains bounded carrier text, client graph, and result.
Repeat calls compare carrier bytes and clone result metadata without reparsing or
serializing the source. Changed content/base replaces that entry; changed resources
or limits require a new provider. Concurrent callers serialize preparation through
a mutex. Cancellation is checked between bounded stages and targets; it does not
preempt synchronous client calls or a thread waiting for the mutex. Dropping the
provider releases its prepared entry; returned inventories own their values.

The carrier crosses from the SDK exact-value arena into client-owned parsing once
per preparation. Result target JSON is copied into SDK arenas once; cache hits
share those immutable values while cloning strings/containers. This is not a
zero-copy claim. `Options` separates carrier/result budgets from the client's
per-resource and aggregate graph budgets. These are product policies.

See the maintained [public consumer](../../examples/openapi-inspection-consumer)
and [decision/qualification record](../../docs/openapi-inspection-slice.md).
Run `python3 tools/verify-openapi-inspection.py --archives --wasm` using Rust 1.99,
wasm-bindgen 0.2.129 and the repository's TypeScript tools. This qualifies native
Rust and a thin TypeScript/Node-Wasm probe; a production npm API, real browser,
Worker and service qualification remain separate work.
