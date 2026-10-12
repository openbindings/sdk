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

[`examples/recovery.rs`](examples/recovery.rs) is a runnable application recovery path. `Partial` retains
the inventory and its limitation; `NeedsRepair` retains the editable draft and
full document assessment. The caller retains its source throughout. Selecting a
known target from a partial result is an explicit application decision, not a claim
that the inventory is complete. Typed cancellation, unsupported-kind and unreadable
refusals are shown without automatic retries. These application outcome types are
example code, not additional SDK entry points. `Ready` means the authored document
passed core conformance assessment; inventory completeness and correspondence to
the actual source remain separate claims.

The test kind treats `input` as an optional framing hint and only establishes
boolean schemas. It leaves every other hint unprojected, including malformed ones,
while retaining the admitted target and unchanged source. A real adapter must
qualify its own projection semantics. The companion does not acquire a general
schema evaluator merely to inspect sources.
