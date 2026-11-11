# Contributing

Work against `main` in a short-lived branch and submit a pull request. Source
changes use squash merge after the required SDK qualification checks pass.
Repository identity is independent of implementation language; today Rust owns
the semantic engine and TypeScript exposes a first-class API through Wasm.

Follow the documented core/optional-package boundaries in docs/architecture.md.
Run the source replay in docs/REPLAY.md. The CI matrix verifies native targets and
real browser hosts; its definitions do not imply unexecuted jobs have passed.
Keep diagnostics focused on documented semantics and real callers; tests follow
the rules below.

Package publication, version selection and consumer migration follow RELEASING.md
and require their own release decision. Do not publish from this source-import
workflow. Historical benchmarks retain their workload and host limits.

## Diagnostics

An SDK error's or diagnostic's `Display` and `Debug` hold fixed SDK text plus
SDK constants: rule IDs, governed codes, the supported specification line, field
and limit labels written in SDK code, limit numbers, byte offsets, lines and
columns. Document content,
caller-supplied values (URLs, URIs, names, payloads, versions), provider and
evaluator output, and anything derived from them are reachable only through
fields, accessors and serialization the caller invokes. Escaping or truncating a
value does not make it value-free.

A wrapper prints its own layer and exposes its cause through `Error::source()`,
never both, so `{:#}` and `anyhow` chains print each layer once. A native cause's
own formatting remains the provider's responsibility; apply the application's
disclosure policy before printing that chain.

A value type's own `Display` (`Pointer`, for example) is the rendering a caller
asked for, not a diagnostic, and is out of scope. A location attached to a
diagnostic is in scope: `SourceLocation`'s `Display` prints line, column and byte
only; its pointer stays reachable through `pointer()` and its public field.

## Tests

A test protects a guarantee: something a caller, the OpenBindings specification,
a published interface, or a documented limit or default relies on. Every test must
earn its place: it fails for a plausible defect that the other tests would let
through.

**Before adding a test**

- Name the guarantee and the defect only this test would catch. If an existing
  test covers the guarantee, add a case to it instead. Check other crates' tests
  too.
- A fixed bug gets a case for the outcome the caller saw, in an existing test
  where one fits.
- A change no caller can observe (a cache, a capacity estimate, a faster path, or
  a mutant that changes nothing visible) needs no new test.

**Writing a test**

- Test through the public API. Test an internal component directly only when it
  has a contract of its own (a parser, decimal arithmetic, the pattern language)
  and the public API cannot practically reach the case.
- Assert outcomes, not mechanisms. Do not assert exact internal costs, internal
  counts, sharing, struct sizes, cache hits, or which path ran, unless the value
  is a documented limit or default, the boundary of one, or a route
  [the maintainer guide](docs/maintainers.md) requires a test to pin.
- Keep it as small as its guarantee: the smallest input that shows it, one
  guarantee per test, no measurement or reporting code.
- Name it for the behavior it protects, not for the review, job, commit or former
  design that prompted it. `tools/verify-product-labels.py` rejects such names.
- Test each behavior in the layer that implements it; other layers test what they
  add.
- Keep one-off evidence (benchmarks, corpus runs, probes made for a decision) with
  that decision, not in the suite.

**Changing tests**

- A change may delete or rewrite a test that pins a mechanism or catches nothing
  the others miss, as long as the guarantee stays covered. Name the covering test
  in the commit message.
- When a guarantee changes or goes away, its tests change or go with it.
- Never weaken a test of a guarantee to make a change pass.
