# SDK support and compatibility

The Rust-backed SDK is an unpublished `0.2.0-alpha.1` working draft. Use an exact
source revision or verified local archives. The legacy npm `latest` package is a
different implementation; see [migration boundaries](migration.md) before
replacing an existing dependency.

| Environment | Supported path and qualification |
| --- | --- |
| Rust | MSRV 1.88, edition 2024; independent native/Wasm MSRV CI. Development toolchain 1.99; Linux, macOS, and Windows native CI. Core has no mandatory async runtime. |
| Node | ESM on Node 22 or later; CI pins 22.19.0. Read the exported Wasm asset and initialize explicitly. |
| Browser | Desktop Chromium and WebKit with explicit initialization; use a Worker for potentially expensive work. Firefox, mobile devices, and particular framework/bundler versions require their own qualification. |
| Worker service | Local workerd with a compiled Wasm module and initialization in an allowed request context. This is not evidence of deployed Cloudflare latency, quotas, or service availability. |

The 256 KiB small-stack document regression is qualified on the development
toolchain. On the same stack, release builds on Linux, macOS and Windows prepare
value contracts whose patterns nest to the default `pattern_depth` and check
values against them. The evaluator sizes its evaluation depth, stack guard and
unevaluated walk limit from `Limits::stack_bytes` (256 KiB by default), and
release builds on the same hosts prepare, check and explain values as deep as
it admits for the 39 recursive contract shapes `evaluation_stack` covers,
unevaluated keywords included, on threads of 192 KiB, 256 KiB and 1 MiB, and
prepare the longest unevaluated annotation walk of each kind it admits with a
fifth of a 192 KiB, 256 KiB or 512 KiB thread already in use (`walk_stack`);
the derivations were measured from 128 KiB to 2 MiB. The static depth bound models
the library's frames, and a contract holding an unevaluated keyword is also
guarded by measuring the stack its pass uses. Unoptimized code needs more:
compiling 32 nested quantified groups takes up to about 1 MiB of stack in debug
builds, and debug evaluation frames exceed 256 KiB at the default depth, so
debug runs ignore both tests and `tools/verify.py` runs them in release builds.
Older supported compilers may need larger stacks in debug builds. MSRV CI runs
all workspace tests except the exact stack-margin tests selected by [this
inventory](../tools/stack-margin-tests.json), with reasons and scope.
Development-toolchain jobs retain the workspace stack tests.

The Rust and TypeScript APIs use the same engine and outcome meanings. They
follow their languages' ownership conventions: Rust uses ordinary ownership and
borrowing, while TypeScript owners support deterministic `dispose()` and
`Symbol.dispose`. Neither an owner count nor disposal proves that process memory
or Wasm capacity immediately shrinks. See the [API contracts](api-reference.md)
and [TypeScript guide](../packages/typescript/README.md).

Document conformance, parsing, and operation value validation are distinct jobs.
The optional built-in evaluator has a declared JSON Schema capability profile;
unsupported capabilities return no-verdict. Rust applications can supply their
own evaluator and qualify it with the evaluator test-support crate. TypeScript
custom evaluator callbacks are not currently provided. The complete boundaries
are in [CAPABILITIES.md](../CAPABILITIES.md).

For untrusted request validation, use `validate`, the cheap check. `explain` is an
explicit per-call choice for callers that accept library allocation; it allocates
in proportion to inputs and explains only values within half the depth limit.
An empty problem list never turns an established failure into success.
See the [evaluator policy](../crates/openbindings-json-schema-evaluator/README.md).

The default evaluator prepares a contract that reaches a missing resource with
an empty stand-in for it; a pass that reaches the stand-in gives no verdict
naming it. It refuses preparation for a contract in which an
`unevaluatedProperties` or `unevaluatedItems` keyword's annotation sources
reach a stand-in, a reference to a fragment of a missing resource, or a static
reference defect (an absent pointer target or anchor, or a malformed
fragment). As conservative preparation, it also refuses a contract that reaches
a pattern invalid as an ECMA-262 regular expression with Unicode semantics
(`schema-pattern-compilation`), or a schema that could apply again without
consuming the instance, such as one whose `$ref` names itself
(`in-place-cycle`). A preparation refusal withholds a verdict for every value,
including one whose validity does not depend on the refused part. The default
evaluator never produces Undefined. Missing resources, unsupported features and
exhausted limits keep their distinct refusal reasons.

Preparing a schema from an untrusted party can take more memory than a host
may want, as with any JSON Schema library: the evaluator bounds what the SDK
itself reads and writes, not the library's compiled program (see the
[evaluator limits](../crates/openbindings-json-schema-evaluator/README.md#limits)).
Prepare such schemas in an isolated worker or process with an operating-system
memory limit. Checking values against a trusted schema stays bounded by the
evaluation depth limit and, when set, the work cap.
