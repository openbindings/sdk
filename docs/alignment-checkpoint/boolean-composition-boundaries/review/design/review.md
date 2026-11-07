# Boolean composition boundaries — bounded design review

**Conditional GO for a small measured implementation; NO-GO for deleting the current polls alone.** Reviewed SDK `0142eaa56ca2c8fd50716e943aeef05931bee19b`. This is a deliberate SDK accounting/boundary change, not a count-preserving optimization. This review used source inspection only: no SDK edits, builds, tests or performance runs. The earlier one-tick wrapper and inline experiments remain discarded.

## Authority and scope

Pinned core §9 requires attention to size, matching and recursive-traversal exposure; it does not specify Boolean-operation accounting. `WorkControl::check` is a cooperative boundary, not a callback-frequency or wall-clock guarantee. The public evaluation contract preserves a verdict established before cancellation is observed, including incomplete Explain after F. `evaluation_steps` and shared SDK work are scoped work policies, not exact CPU certificates. Nothing here authorizes skipping children, erasing a local limitation, changing a semantic oracle, increasing caps, or weakening shared-attempt failure propagation.

The coherent policy is: **charge traversal progress and nontrivial symbolic analysis; terminal Boolean algebra is free.** Intercept only the existing symmetric AND/OR annihilator, identity and equal-formula rules at their public-private wrappers. The operands have already been computed. These rules inspect no BDD node and create no cache/storage state. Keep `apply` entirely unchanged, including its initial tick, recursive calls and their terminal visits, nonterminal cache hits, and Xor. Keep `not`, quantified recursion, effect identities, witnesses, annotation gating, child requests, dependency guards and numeric limits unchanged. Do not generalize this to polling strides or other helpers.

## Mandatory progress repairs

Add an unconditional `graph.tick()?` at the start of each of these four loop bodies, before work/mutation for that iteration:

| Current source | Loop | Why this is necessary |
| --- | --- | --- |
| `semantic/execution/mod.rs:353` | `complete_application`: chained property/item annotations | AND is presently the only per-entry poll. |
| `semantic/mod.rs:827` | `annotations`: source properties | AND/OR can both be terminal; ordinary storage charge does not poll. |
| `semantic/mod.rs:834` | `annotations`: source items | Same issue. |
| `semantic/counts.rs:33` | `Matches::add`: uncertain distribution slots | The initial add tick and not tick do not cover a growing slot loop. |

A symbolic iteration may now pay a traversal tick plus nontrivial analysis. A concrete iteration may pay fewer units than today. This is intentional and must be recorded; do not infer unchanged exact thresholds. Preserve all storage charges, and do not attempt conditional compensation based on whether a formula call happened to charge.

## Remaining composition/progress map

`call-sites.json` records every production AND/OR call in these modules, with exact source text/line and pinned hashes. The corresponding progress owners are:

| Calls | Existing boundary that remains |
| --- | --- |
| Logical all/any/one/not/conditional folds, reference conjunction | `Frame::advance` ticks every phase iteration; child entry/request checks remain. Conditional selection has only three parts. Annotation merge uses the repaired loops above. |
| Object property/pattern/additional/propertyNames/unevaluated folds | Frame phase tick; dependency/member loops have explicit ticks; each child evaluation retains its entry. Finite returned-child formula batches do not conceal another unbounded loop. |
| Array items/contains/unevaluated folds | Frame phase tick; item scan explicit tick; contains uses repaired Matches and existing per-slot `within` tick. |
| Pattern result in `begin` | Existing entry and pattern-search guards, followed by one finite composition. |
| `unknown_schema` annotation maps | Property `units` performs checked work; every item calls `atom`, whose context/predicate work checks even on a reused effect. |
| `discover_support` memo annotation folds | Properties have name work; items explicitly tick. Outer support traversal also ticks. |
| `recursive_cut` maps | Properties call `units`; items explicitly tick; fresh atoms retain checks. |
| `conjunction` | One finite AND and repaired annotation loops; reached under the frame boundary. |
| `choose` / `implies` | Finite operations and unchanged `not`/apply boundary; their callers retain traversal boundaries. |
| `quantify` combination | Every recursive visit ticks before terminal/cache checks. Underlying nontrivial apply recursion remains unchanged. |

Evaluation entry checks remain. `complete_application` also performs checked context work after its result processing. This is a targeted composition audit, not a new universal maximum-cancellation-latency proof: existing independent State scans, BTree operations, dependency calls and destructor behavior retain their existing policies. No new promise that every machine operation polls is appropriate. Removing a sample inside a constant-time wrapper does not remove recursive stack observations in `apply`/quantify or scheduler checks.

## Shared preparation attempts

A terminal fast return must, when `Graph.preparation` is present, call that existing control's `charge_work(0)` and map errors through `preparation_control_inability`. It must not use `check()` (one unit for the SDK Attempt), `Graph::tick`, or a new ledger. With the actual Attempt, zero units still detect cancellation, sticky reservation exhaustion and TLS work exhaustion in the existing precedence; a healthy allowance exactly at zero is different from an already-spent allowance. The enclosing Attempt's final check remains necessary too. Do not retain the control or create a fresh allowance.

Without an enclosing preparation control, the pure terminal result performs no ordinary WorkControl/stack poll and spends no graph unit. Public entry/progress checks still observe cancellation. Existing Graph errors propagate with `?`; this change must not catch errors or make ignored private Graph failures into success. There is no need to invent a new universal sticky-Graph error model for this slice. Public pre-aborted calls still refuse; a later unobserved cancellation may coexist with an established verdict under the documented contract.

## Controls, expected value and decision boundary

`controls.json` freezes 12 future groups covering terminal algebra, new direct boundary behavior, actual nested Attempt failures, all four loop repairs, known progress owners, eager local limits, recursive/correlated results, Explain and measured selection. These are proposed qualification controls, not executed evidence. Exact prior callback counts and work thresholds are expressly retired for this slice; preserve their earlier receipts and recompute exhaustive stops/exact-one-below tests from the new counted traversal. Keep original funded semantic/location expectations and all configured caps unchanged.

This can be a real simplification rather than blanket control throttling: the trivial operation loses artificial proof work while growing loops gain explicit ownership of their own progress. Expected benefit is limited: true array folds and object validity/selection folds lose checks, but leaf numeric/text work, raw lookups, context handling and frame dispatch remain. Uncertainty/annotation-heavy work may become slightly more expensive from explicit loop ticks; branch duplication before nontrivial apply can also cost time. The previous count-preserving wrapper regression is evidence to measure, not proof this distinct policy will succeed.

Approve implementation only at this narrow scope, then keep it only if exact semantic/resource controls and uninstrumented selected measurements support it. Otherwise retain the current path. No G6/performance acceptance, broader polling policy, second evaluator, or physical-stack promise follows from this design review.
