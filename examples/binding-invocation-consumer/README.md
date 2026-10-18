# Binding invocation consumer

An external, unpublished qualification package using public SDK APIs. Run
`python3 tools/verify-binding-invocation.py --archives --wasm` from the SDK root
with the cached Cargo, wasm-bindgen and TypeScript tools available. The verifier
uses the shared target tree, records commands and hashes, and never publishes.

`Catalog` reuses the existing `interface-synthesis-consumer` fixture's inspection
and synthesis implementation. Its invocation implementation independently reads
literal unit IDs and dispositions, records the target it executes, counts effects
and echoes exact input values. Tests compare those observations to authored
expectations rather than to generated coverage. They choose a binding explicitly,
convert it once, and reuse its source and carrier across calls. A separate path
passes a hand-authored request directly to invocation. The operation consumer
holds its invoker in an `Arc`, retains `invoker.prepare("s0.u0")` and invokes that
handle twice, counting one
compilation per declared direction while supplying `Attempt::new()` on every
call. The handle fixes the operation; each attempt selects only its binding and
optional opaque context.

The existing `urn:example:capability-catalog:v1` default remains arbitrary-JSON
echo: each accepted input produces that same exact value, in order. A lossy unit
still has that upstream domain; synthesis advertises a narrower null input schema.
Binding invocation receives no operation schema and adds no such validation.
Excluded, invalid, unsupported and dependency units cannot be invoked. A string
binding content selects exactly `(target, id)`; an object
`{"scope":"alternative"|"projection","id":"..."}` selects that exact scope and
ID. The invoker validates the complete unit identity/scope/disposition model,
rejects duplicate `(scope,id)` pairs and wrong typed present fields, and never
chooses by array order. The optional unit `interaction` field adds explicit test-only modes:
`unary` echoes one value and closes input; `server` repeats one input seven times;
`count` counts inputs and returns one count; `empty` closes input and completes
with zero outputs. Catalog and `invoker-only` declare no preflight hook, so their
preflight runs the SDK's checks and makes no prediction. These are fixture
semantics, not an OpenAPI kind definition.
The shared [fixture authority](../interface-synthesis-consumer/README.md#fixture-authority)
requires all four interface operations (inspection, strict/coverage synthesis and
invocation) to refuse wrong-typed or unknown interactions throughout the artifact.
Maintained consumer cases cover those refusals and all six omitted/explicit mode
choices across represented/lossy target, alternative and projection bindings.

`invoker-only` has no inspection, synthesis, HTTP, optional evaluator or executor
dependency. The full consumer separately qualifies borrowed non-Clone providers,
conditionally Send native invocation, and local Rc across a real Pending boundary.

The Node/Wasm bridge is qualification plumbing, not a released TypeScript interface
package or a browser claim. One scoped Rust host task owns the provider, request
and borrowed SDK halves. It has one command slot in each direction; all operation
values flow through the same bounded SDK lifecycle. This host task solves the
wasm class ownership boundary and is not a mandatory SDK executor. The thin
TypeScript wrapper provides async writes, an async output iterator, cancellation,
disposal and AbortSignal forwarding. Iterator return disposes the attempt. Exact
values cross JavaScript as JSON text; a failure retains its exact portable record.

The Wasm-only provider retains Rc across a JavaScript Promise. The host probe
checks commands and Rust provider/driver/token owners are released after disposal
and cancellation, and that a late host Promise resolution cannot resume cancelled
work. Dropping Rust work does not cancel an externally owned JavaScript Promise;
the test settles that Promise separately. A bridge remains owned until terminal
observation or disposal, so callers abandoning it must dispose it.
