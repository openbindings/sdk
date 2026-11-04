# Guarded AST construction checkpoint

The private AST stage now has independent design and implementation/resource
approval. It remains test-only while complete pattern construction is unfinished.
There is no new public API or production evaluator path in this increment.

The stage guards the exact borrowed form, computes checked target-sized work and
allocation reservations, pays them before entering a fresh backend parser, and
checks the enclosing control again before returning ownership. Parser and backend
diagnostic ownership are released without formatting. AST destruction, including
after late cancellation, is prepaid. A successful result owns only the AST and
its scalar inventory. Local grammar/depth/representation/layout refusal remains
distinct from an exhausted or cancelled preparation attempt.

The source-derived work schedule includes byte scans, typed transfers, vector
growth, nesting traversal and allocating iterative destruction. Independent design
review corrected the visitor allowance to include descend/continue helpers. The
fixed 1,024-unit checked-schedule computation is charged separately. Public layouts
are checked against actual types; private dependency layouts remain explicitly
pinned and require qualified target/build reproduction before activation.

Validation at the source hashes in `ast-stage-source-hashes.json`:

- Eight stage test groups pass on Rust 1.99 and minimum Rust 1.88, including exact
  and one-below limits, overflow/sticky causes, no parser entry after refused
  precharge, late cancellation, independent retry and all unchanged Linear forms.
- The current evaluator library suite passes 129 tests, with two existing
  release-only stack tests ignored in that library invocation. Strict all-target
  evaluator lint passes.
- The actual source allocation/event probe passes 4,335 rows on each Rust
  version, with identical results. Actual Wasm32 under Node 22.23, Chromium
  156.0.8078.4 and WebKit 27.2 passes the same 4,335 observed rows. There is no
  allocation undercharge, retained ownership, schedule mismatch or changed event
  sequence. These include 48 independent success/cancellation/precharge lanes.
- Independent implementation review adds 151 allocator/cancellation/exact-limit
  observations on each Rust version: all pass, with identical results. These
  counts overlap other evidence and are not additive coverage claims.

The initial parent unit run had one incorrect hand-counted test-span length;
the expected endpoint now uses the actual input length. Initial standalone probe
module/export/environment mistakes and the independent driver's non-Sync counter
mistake are retained in the coordinator. None changed an SDK semantic expectation.

Design and implementation reviews are saved alongside this file. Complete raw
commands, source/event instrumentation, inputs and observations remain in the
coordinator's `receipts/2026-11-02-preparation/pattern-ast-work/` and
`pattern-ast-stage/` directories. `ast-stage-qualification.json` preserves the
bounded results and identities in this branch.

Next: checked iterative AST metadata for HIR reservations and prospective
translation depth; actual HIR/NFA phase admission and target qualification;
once-prepared immutable programs; complete aggregate/default-workload replay.
The ordinary evaluator still includes the outgoing compiler. Old APIs have no
compatibility protection, and full compiler/default-switch approval remains open.
