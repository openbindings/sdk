# Retained original pattern readings: independent implementation review

**PASS for this bounded increment. No unresolved correctness finding in the reviewed change.** Production source is based on 155f073 with the final explicit Unsupported Arc-layout reservation; exact reviewed files and executable artifacts are pinned in `source-hashes.json` and `artifact-hashes.json`. No production file was edited by this reviewer.

This closes retained parsing/readings and shared Simple ownership. It does **not** close full once-prepared immutable VM programs, demanded-state evaluation, the general PikeVM allocation certificate, ordinary schema compiler accounting/retirement, or whole SDK performance qualification.

## Source review

`Plan` retains readings keyed by `(arena address, original operand node)`. The retained environment owns those arenas. Both `pattern` values and exact `patternProperties` member-name operands use that same identity at preparation and evaluation. Profile activation still passes through `Interpreted`; original reachability/typed query discovery is unchanged. No preparation control, instance or mutable search state is retained.

Ordinary value patterns share an Arc by normalized form, matching the pre-existing distinct-form byte/memory policy. Parsing remains once per original discovered operand, with cumulative temporary-reader reservations even when a duplicate is subsequently discarded. Name operands remain individually counted and retained. Sharing storage does not share invalid/unsupported predicate effects: those still derive from original schema/keyword and exact tested value. Unsupported readings are not interned in the normalized-form index.

Runtime Simple state borrows the Arc. VM compilation remains lazy and local to one Run, and each stored VM owns its own Cache. Runtime no longer parses original text. A missing prepared entry yields EvaluatorFailure. Typed invalid, unsupported, expanded-form/VM-size, unpaired-string and cancellation paths remain distinct. The existing eager traversal can still return a terminal Limit beside `anyOf: true`; this increment makes no stronger masking claim.

Reader work/allocation is charged before parsing; Arc and both original-operand/form maps are reserved before retention. The final adjustment explicitly reserves `size_of::<Reading>() + 64` for an Unsupported Arc, rather than relying on unused slack in reader reservation despite the enum's large Linear variant. No observed overrun prompted that clarity improvement. Linear form clones remain covered by the existing lexical reader reservation; there is no new compiled-program construction in preparation.

## Independent exercised controls

The public probe forces original execution with an externally reached 2019-09 root, rather than assuming the outgoing full-schema backend used the changed code. All outcomes are independently specified and checked in both Outcome and Explain modes:

- Simple calls at 128 evaluation work units, repeated after preparation and original-owner disposal; native private Plan controls independently show parser work alone exceeds 1,000 units.
- Invalid expressions yield Undefined only when decisive; supported-syntax inability remains NoVerdict; non-strings and decisive ordinary branches preserve S/F.
- Two references to one invalid predicate yield F under oneOf; two distinct invalid predicates remain U. Valid normalized-equivalent patterns retain their expected oneOf result.
- Concurrent repeated VM property-name calls retain separate search state and correct accepted/rejected property values.
- All 29 observed VM evaluation checkpoints cancel; all 47 warm Simple preparation checkpoints and 47 each for invalid/backreference preparation cancel. Clean retries succeed. The initial harness counted cold-cache preparation work and then replayed warm calls; that harness mismatch was corrected by warming before measuring, not by changing an SDK assertion or outcome.
- Zero construction work/allocation caps refuse and retry; Unicode property matching, unpaired instance strings and non-scalar authored patterns preserve their respective boundaries.
- Ordinary duplicate forms fit a one-form byte cap; repeated property-name operands exceed that same cap, preserving existing policy.
- 100, 400 and 1,024 unique unused non-Simple patterns all prepare under defaults and accept the empty object. This confirms the narrower design avoids the measured eager-construction hazard.
- Expanded-form and actual VM-capacity failures remain Limit, not Undefined; non-string values do not demand them. Source/depth caps refuse. Opaque const/unknown content and draft-07 reference siblings do not create pattern obligations.

`public-probe-results.log` records the run. Independently reran all four prepared-artifact native tests and all six pattern-correctness groups, including generated ECMAScript/Unicode differential and reviewed properties/assertions; all passed. Parent separately reports current/MSRV full evaluator, required 1,546 values, host/browser and package gates; those larger gates are not represented as independently rerun here.

## Remaining certificate and implementation gates

The unchanged per-call PikeVM reservations are not promoted to a universal allocation proof by this review. Before retaining immutable VMs, audit normalized AST/HIR and NFA compiler scratch, retries/final compaction, and per-call sparse/slot/epsilon-stack allocation together. Preserve the default-budget unused-pattern controls and the original caller outcomes. The ordinary compiler gate remains open until a proven narrow lane or prepared original evaluator replaces it.
