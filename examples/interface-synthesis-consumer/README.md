# External synthesis consumers

These version-only consumers exercise public APIs from a separate Cargo workspace.
Run `python3 tools/verify-interface-synthesis.py` at repository root. Add
`--archives --wasm --tsc /path/to/typescript/bin/tsc` to qualify extracted packages
and an actual local Rust provider across a JavaScript Promise in Node/Wasm.
`--performance` runs the fixed release workload, writing CSV rows in the run log.
No registry publication or OpenAPI adapter is involved.

`Catalog` implements independent inspection and synthesis over shared private
analysis. `synthesis-only/` builds a skeleton provider with no inspection dependency.
The verifier checks that absence in Cargo's resolved dependency graph. Neither
provider requires a runtime registry, boxed trait object or service executor.

## Fixture authority

The code-defined kind `urn:example:capability-catalog:v1` is deliberately small.
It is a qualification fixture, not a public binding specification. Content requires
`units`, an array with unique `(scope, id)` pairs and nonempty Unicode scalar IDs.
Omitted scope means `target`; omitted status means `represented`. Supported scopes
are target, alternative, projection and dependency; supported statuses are the
five interface dispositions. Unknown/non-string identifiers and unknown scope/status
labels refuse as wholly unreadable. The optional `unknownRemainder: true` declares
that the observed inventory cannot establish completeness.

An omitted `interaction` means `echo`. An explicitly supplied interaction must be
one of `echo`, `unary`, `server`, `count`, or `empty`: echo returns each input,
unary echoes one input and closes input, server repeats one input seven times,
count returns the number of inputs after half-close, and empty closes input and
completes without output. Wrong-typed or unknown interactions make the entire
artifact unreadable for inspection, strict synthesis, coverage synthesis and
invocation, including when the malformed unit would not be selected or emitted.
Authoring and execution readers enforce this fixture authority independently.

For this fixture, a represented unit offers its declared interaction. Excluded
units are disallowed by this kind's own rule; invalid units declare malformed
upstream units. Unsupported units admit an interaction the provider cannot emit.
Lossy units retain a sound interaction while advertising only JSON null input,
a deliberate narrowing of the upstream arbitrary-JSON input domain. Dependency units declare
consumption points for the equivalent operation and have no binding. These literal
semantics are tested independently of the facade's generic correspondence checks;
the fixture makes no claim about arbitrary schema projection or execution.

Source names honor caller-supplied names exactly. Unnamed input `i` gets `source{i}`,
adding underscores until it avoids all reserved explicit/generated names. Emitted
operation/binding/dependency keys are `s{i}.u{ordinal}`. Source references retain
original unit IDs, so Unicode identifiers never become illegal operation names.
Target binding content remains the plain ID string. Alternative and projection
binding content is an object with `scope` (`alternative` or `projection`) and
`id`. Execution of the shared fixture exposed why that distinction is required:
the same ID may occur in several scopes with different semantics, so a string
alone cannot identify all three. This preserves default target echo bindings
while making every generated executable scope identifiable without operation-key
inference or source-order selection. Dependencies remain without bindings.
For fixed input order and source state naming is deterministic. Equal explicit
source names with equal carriers may share the emitted source; both input indices
still have distinct coverage. A single-source provider refuses arity before analysis.

Inspection lists target-scope admitted units, including a synthesis-unsupported
unit; projection limitations do not hide inspectable targets. Neither interface calls
the other. The core SDK remains independent of this source semantics.

## Recovery and persistence

`examples/recovery.rs` demonstrates the ordinary strict call, an `Incomplete`
branch, and recovery of the checked document plus report from that single call.
`tests/consumer.rs` covers shared non-Clone state, borrowed/unboxed Send futures,
all scope/disposition combinations, source composition and independent fixture
soundness checks. The Wasm-only provider retains Rc across a real Promise and drops
request/provider handles before exact result encoding. The TypeScript probe keeps
the returned JSON text for persistence; its JSON.parse view is only for summary
fields, since JavaScript numbers cannot preserve every source numeric token.
