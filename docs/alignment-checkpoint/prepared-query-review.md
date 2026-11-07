# Independent prepared original-query review

Bounded **PASS** at `511a1bf`, 2026-11-03. Source hashes and the four reviewed files are retained in `source-hashes.json` and `original/`. No production edits. This is the immutable query artifact increment, not a backend replacement or closure of the remaining ordinary compiler allocation gate.

## Ownership and semantic invariants

`check_from` already performs the same original `SchemaSpace::query` calls to discover target/anchor closure. It now retains each typed result in the completed Plan. `(original node, ReferenceKind)` is sufficient within that Plan: the owned environment fixes the original source, resource/base, profile, supplied snapshot, and original position. Distinct nodes with equal URI spelling do not collide; distinct reference keywords on the same node do not collide. Nothing is shared between independently prepared environments. `Arc<Result<Target, QueryFailure>>` stores only original target identity, optional dynamic name, and typed refusal; it retains no preparation control, attempt, instance, or effective evaluation scope.

Only the initial static lookup is cached. Both ordinary reference evaluation and transparent alias normalization still select `$dynamicRef`/`$recursiveRef` targets from the current effective Context. The preparation queue still closes over static targets plus activated dynamic/recursive candidate anchors, including candidates encountered after a name was used. Profile activation and draft-07 sibling suppression use the same `Interpreted` view in preparation and evaluation. Non-scalar reference strings retain their existing local undefined path without demanding an absent query-cache entry. Ignored positions remain ignored; the actual-meta Plan obtains its own checked query set at its actual original entry.

Malformed/missing/non-schema targets are retained as structural results; this change does not turn them into blanket preparation rejection. Existing admission conflicts still reject. An unexpected missing cache entry yields `EvaluatorFailure` and cannot trigger a hidden lookup, invented Undefined result, or a new resolution environment. Cycle recognition and annotation-support discovery now consume the same prepared results.

## Construction and per-call bounds

The existing size-based query reservation remains before `space.query`. The additional fixed 1,024-byte reservation precedes HashMap insertion and Arc allocation; the key/value and Arc control/result are fixed-size, with separately reserved query-owned text. A missing-resource URI is now cloned for the separate missing-resource set, and that new copy is explicitly reserved first. The insertion-only map owns results until the Plan drops; temporary results drop with a failed construction. There is no partial Plan publication.

Per-call access performs a graph tick before borrowing the map and cloning the Arc; it performs no URI parsing or string cloning. Runtime creation of unavailable-effect identities and diagnostics retains its own work/allocation charges. This removes repeated query spelling work for resolved targets, not all work involving a URI. Dynamic scope, annotations, patterns, and repeated fixed syntax admission remain separate costs. This review does not claim new general backend allocation bounds.

## Independent controls

`public-probe.rs` uses the actual current public core/evaluator libraries. Its external entry explicitly declares 2019-09, routing these cases through the original semantic engine rather than relying on the ordinary 2020-12 compiler. The linked artifact identities, exact compiler invocation and library fingerprint are retained in `public-probe-build.json`/`artifact-fingerprint.json`.

The probe passes Outcome and Explain controls for:

- The same relative spelling under two different original bases.
- One prepared shared dynamic-reference node reached under string and integer outer scopes; repeated alternating calls succeed for homogeneous trees and fail for mixed trees.
- The equivalent 2019 recursive-anchor scope controls, again over repeated calls.
- A pointer into an embedded resource with a profile/base switch, plus repeated independent preparations with the same URI naming true/false/true resources.
- `$ref` and `$dynamicRef` with different targets on one node.
- Typed malformed-reference Undefined, genuinely unavailable content, and decisive branches masking each.
- Unknown-keyword contents and draft-07 reference siblings staying opaque.
- Zero preparation allocation allowance refusing, followed by successful preparation and evaluation after all input owners drop.
- Pre-cancellation and clean retry, plus **all 194 observed public dynamic evaluation cancellation boundaries**, each returning Cancelled. The retained Plan remains usable afterward.

The two new production prepared-query tests were independently rerun, including the 10,000-byte resolved URI at 128 evaluation steps. Four existing native integration targets were also rerun: current reference obligations (including recursive/coordinate controls), custom profiles, dynamic references, and supplied dialects. All pass; exact group counts and output are in `native-targeted.log` and `native-integration.log`. Parent-reported MSRV/browser/full-kit runs are additional evidence, not relabeled as independently executed here.

No blocker found. Keep the existing long-reference preparation and performance gates unchanged: this increment deliberately does not fix repeated syntax checking, ordinary concrete execution cost, or semantic Explain witnesses.
