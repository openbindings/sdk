# Current invocation consumer

This external package demonstrates the current original-document binding and
operation APIs. Its local provider echoes input values for the two illustrative
kinds in the source-pull consumer. A binding's `data.item` selects the catalog item;
opaque metadata remains exact. The examples use finite known catalog shapes,
not arbitrary protocol descriptions.

The caller explicitly pulls and adopts additions before invoking a named binding.
Operation calls additionally apply the original operation schemas. Preflight is
advisory and separate from execution. A configured context requirement is returned
before input consumption and output; only the caller supplies a later attempt's
context. The provider uses no SDK registry or implicit executor.

Run `python3 tools/verify-invocation.py`; add `--archives --wasm` for extracted
dependencies and a real local TypeScript/Node host. A separate invoker-only package
qualifies the invocation dependency boundary without pulling or an evaluator.
