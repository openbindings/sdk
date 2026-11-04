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
