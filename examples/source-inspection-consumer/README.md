# External source-inspection consumer

This independent Cargo workspace has version-only public SDK dependencies. It
implements a catalog inspector without synthesis or invocation, then uses inspection
to author and assess a document. The fixture kind is defined in `src/lib.rs`; it is
not a proposed OpenBindings kind specification.

From the SDK repository root:

```sh
python3 tools/verify-source-inspection.py
python3 tools/verify-source-inspection.py --archives --wasm
```

The verifier prepares local dependency patches because the packages are unpublished.
Archive mode uses extracted package sources, not checkout imports. The optional
TypeScript probe compiles against generated declarations and executes an `Rc`-holding
Rust provider across a JavaScript Promise through Wasm in Node. It qualifies host
viability, not a released TypeScript capability API.
