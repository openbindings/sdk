# Experimental OpenAPI adapter

This optional crate implements the existing source inspector, interface synthesizer,
and binding invoker roles for **`local.openapi-json-probe.r1`**, a private experiment.
Use `openbindings-invocation::operation::Invoker` for document-based operation selection
and declared value contracts. Core and the published roles do not depend on OpenAPI.
This package does not implement a published OpenAPI binding specification.

The initial profile qualifies finite JSON requests against OpenAPI 3.1.2 descriptions.
The client admits the 3.1 patch family; that is not a claim of testing every patch.
Inspection and synthesis require no transport. Bind application-owned execution once
with `OpenApi::with_transport` to invoke. Local non-`Send` hosts work on Wasm; native
hosts retain `Send` when their futures and captured state are `Send`.

## Source and binding

```json
{
  "kind": "local.openapi-json-probe.r1",
  "content": {
    "document": "{\"openapi\":\"3.1.2\", ...}",
    "retrievalUri": "https://descriptions.example/api.json"
  }
}
```

`document` is JSON **text**, preserving numbers beyond JavaScript's exact integer
range. `retrievalUri` is an optional absolute resource identity. It grants no fetch
permission. Put additional `ResourceInput` values in `Options.resources`. Saving an
OBI preserves its source; independent cold reload must re-supply external resources
at the same identities. The OBI does not embed the whole resource graph.

A binding's content is an exact entry path and lowercase method:

```json
{"path":"/parcels/{parcelId}","method":"get"}
```

Mounted paths identify callable operations. Component pointers, `operationId`, and
suggested generated names do not. Generated core keys may be renamed consistently.

## Invocation

Send exactly one JSON object, then close input. No request is dispatched until input
closes. Zero or additional values, unknown envelope members, undeclared parameters,
duplicate JSON names, or unsupported shapes refuse before effects.

```json
{"parameters":{"path":{"parcelId":"box-7"},"query":{"parcelId":"alias-9"}}}
```

```json
{"body":{"tracking":9007199254740993,"label":"雪"}}
```

An absent `body` means no entity; `"body": null` sends JSON null. Path and query scalar
values remain separate. Complete 2xx JSON produces one exact body value. A verified
empty 204 produces no value. Always drain and check the terminal outcome; receiving
an output alone does not establish success. Complete non-2xx responses produce
`local.openapi-json-probe.r1.http-status` with data `{"status":404}` and no output.
Malformed, incomplete or unsupported successful delivery produces `ERR_EXECUTION_FAILED`.
Cancellation and other post-dispatch failures do not promise rollback or authorize retry.

If effective server URLs are ambiguous, the adapter returns the existing `config.value`
challenge. Supply the selected declared URL in
`context.configuration["local.openapi-json-probe.r1.server"]` and deliberately start a
new attempt. Context applies to that attempt; the adapter neither stores choices nor
retries. URLs select declared destinations and do not allow an arbitrary override.
The opaque challenge `target` distinguishes retrieval identity, operation and effective
choices. It is not a trusted cache key or authority; applications own persistence.

The maintained [external consumer](../../examples/openapi-json-path-consumer) shows
inspection, synthesis, save/reload, explicit evaluator setup, and a finite call helper
that preserves the actual terminal even if `write` reports closed admission. Its thin
TypeScript probe passes JSON as text through Wasm. It is qualification code, not a
published first-class TypeScript adapter API.

## Synthesis and scope

Zero sources produce a skeleton; one source is preserved; composition of multiple
sources is currently unsupported. Generated input schemas describe the mechanical
request envelope, scalar parameter names/types and required flags. Supply a schema
evaluator to the generic operation invoker when invoking those declared contracts.
Source schema constraints and output schemas are not translated in this slice:
coverage reports projection loss. Callable targets, unsupported known targets, and
unestablished inventory are distinguished. Strict eligibility is not full schema fidelity.

The qualified profile uses GET/POST, absolute HTTP(S) servers without variables,
path/query scalar parameters, finite JSON bodies, and supplied references. Forms,
security acquisition, binary/streaming delivery, richer parameter locations, callbacks,
webhook services, ambiguous Path Item merges, and literal non-ASCII path templates
are not implemented. Explicit non-204 no-content success declarations are currently
unsupported. Percent-encoded Unicode paths preserve their spelling; the client gap
for literal Unicode is tracked separately. Error-response bodies/headers are discarded
under the status-only mapping and their lost representation is disclosed in coverage.

`Options` owns immutable client/graph budgets and supplied resources. The adapter
caches one prepared source, charges retained metadata, and holds no cache lock across
host I/O. Source, result, input and body limits are finite. Logical budgets are not RSS
measurements. Host policy belongs to the application; the qualified reqwest companion
follows no redirects and performs no implicit retry or credential acquisition.

The fresh verification entry point is `tools/verify-openapi-json-path.py --archives --wasm`.
Native HTTP and Node/Wasm seam qualification do not establish browser or Worker network
support, package publication, or full OpenAPI coverage.
