# Authenticated JSON landing assessment and deferred performance work

Date: 2026-10-13. Matthew directed that further benchmark work stop and that the
unresolved check and optimization opportunities be documented. The implementation
is ready for PR review with the performance qualification gap stated explicitly.
Matthew subsequently authorized landing the complete SDK stack through the normal
PR and CI process with that gap disclosed. This authorizes landing with the deferral;
it does not regrade AJ20, relax a numerical/resource limit, resume measurement, or
authorize package publication. This record precedes the PR and merge.

## Unresolved performance check

AJ20 remains unpassed under the original [performance policy](performance.md).
The repaired candidate passed all 24 unsecured regression rows and 14 of 16
incremental-auth rows. The two failed rows were two-credential 64 KiB POSTs through
the operation invoker, exceeding their allowances by 0.702 µs for OpenAPI 3.0.4
and 0.293 µs for 3.1.2. The initial run and all later results remain preserved.

The subsequent [measurement resolution experiment](measurement-resolution/report.md)
ended inconclusive: 61/72 eligible controls passed, including four failures when
two labels invoked the same physical function object. Phase Q did not run. The
experiment establishes insufficient demonstrated resolution for this estimator
on this host; it does not establish the cause of the historical failures or prove
that authentication meets its budget.

Further benchmark work is deferred by Matthew's direction. No new host study,
estimator change, qualification run or optimization experiment is active. The
numerical thresholds, resource limits, raw results, case outcomes and independent
reviews remain unchanged. Landing with this disclosed gap is distinct from full
qualification under the original plan. `qualifiedComplete` remains false.

Reopen performance work when a representative consumer exposes material overhead,
a deliberate change targets one of the costs below, or a release needs a specific
performance claim. Set a bounded scope before doing so; no automatic retry loop.

## Concrete optimization opportunities

These are code observations and workload-dependent candidates. None is a measured
explanation of the two failed authentication rows, and no speedup is promised.

| Opportunity | Current work and affected consumers | Direction and constraints |
| --- | --- | --- |
| JSON representation conversion | The adapter reparses SDK request bodies into client `ExactJson`. The pinned client copies JSON bytes into prepared bodies. Client response parsing is followed by parsing into SDK `JsonValue`. Cost grows with payload size. | Investigate a safe bridge or reuse of admitted bytes/views when substantial JSON workloads justify it. Preserve exact values, admission limits, ownership, response checks and SDK/client independence. |
| Operation lookup | Each adapter call scans prepared operations for its path and method. Cost can grow with operation count and target position. | Consider an index built with source preparation. Bound retained index memory and preserve selection semantics. The single-operation auth fixture does not exercise this scaling issue. |
| Source cache | One prepared source is cached per provider. Alternating sources can repeatedly prepare them; preparation holds the cache mutex. Independently allocated equal source text can also require a byte comparison. | Assess per-source provider reuse or a bounded cache design for an actual multi-source/concurrent consumer. Preserve resource limits, cancellation, public-source-only caching and the absence of locks across host I/O. |
| Per-attempt authentication and invocation bookkeeping | Scheme selection, grant validation, owned credential copies, client security-declaration processing and header construction add work. Generic invocation also allocates lifecycle/queue state and manages cancellation and completion. | Profile only if a small-call or high-throughput consumer makes this material. Cache only immutable public metadata; retain all credential scope checks and attempt isolation. Generic operation invocation passes context through rather than acquiring credentials. |

Relevant adapter code: [request and response conversion and operation lookup](../../../crates/openbindings-openapi/src/invocation.rs),
[source cache](../../../crates/openbindings-openapi/src/lib.rs), and
[authentication](../../../crates/openbindings-openapi/src/security.rs).
The client observations concern the pinned revision
`fe09522a9b99a6c6b4918e3de0467b5a638afa5e`.

The direct benchmark consumer intentionally also converts JSON into SDK values.
Its timings therefore do not isolate bare client transport cost. No external API,
credential service, DNS, internet latency or real TLS participates in the main
CPU gate. Existing measurements do not attribute a defect to the client or a
third-party dependency.

## Readiness apart from performance qualification

- Affected Rust tests and lint, extracted-package consumers, TypeScript compilation,
  Node/Wasm execution, dependency isolation and local HTTP cases passed.
- Native and Node/Wasm consumers passed 1,000 mixed authentication lifecycle cycles
  per kind, including failure and cancellation paths.
- Independent Astra and Claude reviews accepted the repaired runtime and contract
  behavior. Their original performance dispositions remain part of the record;
  this deferral is a later project decision, not a new reviewer sign-off.
- On this date, all 83 tested inputs and all 59 product-file entries in
  [the latest functional receipt](qualification-investigation/receipt.json)
  matched the working tree. All 25 recorded commands exited successfully. Product
  paths are unchanged since implementation commit `3315307`; later changes concern
  measurement tooling and evidence. Workspace formatting also passed.

The [receipt extension](measurement-resolution/receipt-extension.json) records the
later measurement harness checks separately. These retained receipts are not a new
full test run. No existing PR was found for `codex/sdk-openapi-auth-json`; remote CI
and merge readiness remain to be established through the usual landing process.

The supported scope remains the existing private 3.0/3.1 kinds, finite JSON
operations, supplied bearer/header API-key credentials and explicit origin grants.
Credential acquisition/refresh, production browser/Worker transport qualification,
broader protocol support, package publication and binding-standard publication
remain outside this slice. Old binding drafts remain excluded as authority.
