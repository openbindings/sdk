# External source-pull consumer

This independently built package supplies two illustrative kinds through the
current `SourcePuller` trait. It depends on published-version declarations rather
than SDK workspace paths; the qualification tool supplies explicit local patches
or extracted Cargo archives. These are local example kinds, not standards.

`example.fixed@1` has no source data and offers an echo operation with string input
and output. `example.embedded@1` reads `data.items`, whose entries carry a canonical
operation `key`, an offered `operation` payload, and binding `data`. The source
check validates this carriage; offered entries must separately satisfy document
rules when pulled. The binding data's `item` selects the corresponding offered key. Optional
`data.partial: true` declares that the supplied catalog is only a partial read.
The examples intentionally have finite, known catalog shapes; they are not general
protocol implementations or validators for arbitrary data under these kind names.

`KindCatalog` implements list/describe and explicit source checking. Its support
claims name only checkSource and pullSource. Describing does not check; checking
uses the same pure catalog reader and does not execute offered operations. The
descriptions constrain selectors to scalar strings (and fixed selectors to echo).
The SDK evaluator may honestly decline lone-surrogate pattern instances; actual
source checking rejects non-scalar keys.

Projection reads source alone. Reuse uses the pinned comparison profile; existing
entry drift compares the exact semantic values the kind would write. Each provider
really suspends once, exercising caller-owned async work without an SDK executor.
The consumer explicitly adopts additions or applies proposals and revalidates.

Run `python3 tools/verify-source-pull.py` from the SDK root; add `--archives --wasm`
to qualify extracted dependencies and the local TypeScript/Node async Wasm bridge.
This fixture's bridge is not a TypeScript SDK role API.

A compatibility run may use `--no-clippy` when that older toolchain has no matching
Clippy component. Its receipt records the omitted lint step; strict lint is still
required separately on the development toolchain.
