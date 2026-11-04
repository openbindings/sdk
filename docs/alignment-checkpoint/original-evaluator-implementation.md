# Prepared original-node evaluator implementation

The independent architecture review selects one private engine over original
schema locations. This replaces the ordinary vendor projection/compiler path;
its API shapes and implementation layout have no compatibility protection.
The default evaluator continues to implement the core evaluation seam.

Evidence: `receipts/2026-11-02-preparation/ordinary-compiler-budget-review/architecture-alternatives/`
records both paths passing 1,546 required values, the unchanged workloads that
the current semantic Plan cannot complete, and the exact root-lock replay. The
review's OE-01 through OE-18 controls govern implementation. Exploratory timings
are not a performance gate or an approved regression allowance.

Implement and review these increments:

1. Remove quadratic concrete `contains` counting. Keep a scalar count of concrete
   matches and a distribution only for uncertain matches; preserve shared-effect
   correlation and evaluated-item annotations. Check exact count boundaries,
   mixed concrete/uncertain results, cancellation and honest bounded refusal.
2. Retain immutable prepared original-node queries, profiles, patterns and exact
   operands under the cumulative construction attempt. Use original arena/node
   and resolution context as identity. Admit syntax once per structural position,
   including unused schema positions; do not reduce an accounting factor without
   removing the repeated work. Separate immutable pattern programs from bounded
   per-call search scratch.
3. Execute ordinary values with compact concrete states, demanded annotations and
   explicit bounded frames. Retain reference-diamond sharing and active recursion
   identity. Promote only real uncertainty into the existing formula machinery;
   any restart shares the same cumulative budget.
4. Return bounded original failure witnesses for Explain. Diagnostic exhaustion
   does not retract an established verdict. Measure retained original storage,
   unused resource pools, replacement lifetime and final cleanup.
5. Force the replacement for the complete current semantic, resource and host
   gates. Restore unchanged large-array/object, depth, long-reference and Explain
   jobs under their current limits. Run the matched performance protocol with
   distinct allocation and timing instruments, then remove the old compiler and
   obsolete adaptation/bounds code.

No default evaluator limits are enlarged by this design. The old backend's
allocation gate stays open until removal or a complete independent certificate;
passing an isolated increment does not close whole-SDK qualification.

## First prepared artifact: original reference queries

`check_from` already queries each activated original reference to discover its
possible targets and dynamic-anchor closure. Retain that typed result in the
immutable Plan, keyed by original node and reference keyword within that Plan's
single environment. This is not a URI-string cache or a new resolution authority.
The stored result includes malformed, missing, unsupported and unavailable
outcomes so preparation does not convert a locally irrelevant defect into a
blanket refusal. Identity-conflict admission keeps its existing rejection.

Per-value evaluation reads the prepared static query under its work/cancellation
ledger; dynamic/recursive target selection still uses the current effective scope.
Arc ownership permits sharing the result without copying its strings or retaining
any preparation control. Reserve construction for the map, Arc and additional
retained string copies before allocation. A missing prepared entry is an explicit
internal inability, never permission to resolve through another environment or
invent a semantic outcome.

Controls must cover the current reference/recursive/coordinate/custom-meta suites,
same spelling in distinct bases, retained calls after dropping the original public
owners, cancellation and low preparation budgets. Record the actual hot-call work
removed; this increment does not claim to fix repeated syntax accounting or the
long-reference preparation gate.

## Second prepared artifact: pattern programs

Separate immutable pattern interpretation/programs from per-call mutable search
scratch. Preparation inventories activated original `pattern` operands and
`patternProperties` names under their actual profiles, retains their readings and
programs by original operand occurrence, and shares equivalent compiled forms
where that removes duplicate construction. Effect identities still use original
schema locations; program reuse never identifies two undefined effects.

Parse/compile only under the cumulative preparation attempt. No borrowed control
or instance survives in the artifact. Keep a simple matcher or immutable PikeVM;
create its bounded cache lazily for a value evaluation under that call's ledger.
Concurrent callers have separate caches, and cancellation cannot poison later
calls. Count emitted forms and admitted pattern memory explicitly; do not enlarge
default allowances or give compiler retries fresh budgets.

Invalid/unsupported readings stay typed local outcomes. A VM representation/size
refusal that does not exhaust construction must remain a dedicated local terminal
Limit, returned only if the pattern is reached. Non-string values do not demand it.
Current eager applicator traversal may still reach it beside a decisive branch;
masking that terminal requires later demanded-state execution and is not claimed
by this preparation increment. It must not become an uncertainty atom.
Actual cumulative preparation exhaustion is sticky and aborts preparation. Missing
inventory entries are evaluator defects, not permission to compile through a new
interpretation during a value call. Existing pattern semantics and corpus
adjudication remain the authority; old cache APIs/ownership arrangements do not.

Review compilation reservation adequacy before moving it into the artifact.
Controls cover retained simple and VM calls without reparsing, patternProperties,
original/profile/snapshot identity, typed invalid/unsupported/masked outcomes,
Unicode exactness, construction/search limits and cancellation, concurrency and
owner cleanup. Force the original engine in targeted controls; ordinary public
engine tests alone can still exercise the outgoing vendor path.


### Implemented bounded pattern increment

Retained readings and shared Simple matchers pass independent review. VM engines
and caches remain call-local for now. The design review demonstrates an eager
construction hazard: 400 minimal first-build reservations exceed the unchanged
512 MiB default before form costs, while 1,024 unused patterns are currently
admitted. Full VM-once compilation therefore requires a different audited aggregate
schedule; it is not completed by retaining readings. See
`receipts/2026-11-02-preparation/original-engine-pattern-review/`.
