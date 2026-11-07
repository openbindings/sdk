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

Projection reads source alone. Reuse and existing schema drift use the pinned
comparison profile, so annotation changes stay with the author. Binding data
compares as exact JSON. Removed operation schemas produce removal proposals;
unfinished comparisons produce explicit omissions or unassessed entries. Each pull
suspends once, exercising caller-owned async work without an SDK async runtime.
The consumer explicitly adopts additions or applies proposals and revalidates.
Its binding data selects source items and needs no other additions, so the
reconciler emits no `uses`. Schema references show their own targets.

To keep part of a pull, choose added bindings, insert their operations when added
and what their `uses` names, then follow schema references to further additions.
Each reached added binding brings its own operation and `uses`. Repeat until
nothing new comes in and validate the edited document. The selective-adoption
test follows an operation's input reference without a hand-written `uses` list.

`src/using_the_result.rs` holds these steps as application code, along with
applying proposals. To take everything a pull offers, call `validate_for`.

Run `python3 tools/verify-source-pull.py` from the SDK root; add `--archives --wasm`
to qualify extracted dependencies and the local TypeScript/Node async Wasm bridge.
This fixture's bridge is not a TypeScript SDK interface API.

A compatibility run may use `--no-clippy` when that older toolchain has no matching
Clippy component. Its receipt records the omitted lint step; strict lint is still
required separately on the development toolchain.
