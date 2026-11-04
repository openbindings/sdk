# Current invocation consumer

This external package demonstrates the current original-document binding and
operation APIs. Its local provider echoes input values for the two illustrative
kinds in the source-pull consumer. A binding's `data.item` selects the catalog item;
opaque metadata remains exact. The examples use finite known catalog shapes,
not arbitrary protocol descriptions.

The native journey lists and describes both kinds, checks a proposed source,
authors the returned entry into a conformant document, pulls, and explicitly adopts
a selected operation and binding. It pulls again without duplicating the adopted
binding, then changes an embedded source in a separate revision and observes a
stale proposal without applying it. The caller selects the adopted binding and
invokes through the operation's original schemas, including an actual reference.

Preflight is advisory and separate from execution. A configured context requirement
is returned before input consumption and output; only the caller supplies a later
attempt's context. Wrong input never reaches the realization. Context is not stored
in the authored source. The provider uses no SDK registry or implicit executor.
These witnesses establish this fixture's effects and capability claims; they do
not certify arbitrary providers or implement a published protocol kind.

Run `python3 tools/verify-invocation.py`; add `--archives --wasm` for extracted
dependencies and a real local TypeScript/Node host. The complete journey runs
natively. The partial Wasm workflow separately checks actual kind descriptions,
Unicode pattern meaning, pull/adopt/invoke, exact values and invalid-input refusal.
A separate invoker-only package qualifies the invocation dependency boundary
without pulling, kind description or an evaluator.
