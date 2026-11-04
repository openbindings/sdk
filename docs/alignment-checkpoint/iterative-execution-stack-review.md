# Independent sampled-stack guard decision

**Approved as a bounded monitoring mechanism, with the conditions below. It does not qualify a hard `stack_bytes` ceiling or close native/Wasm stack safety.** No production edits. A private per-call guard in Graph is preferable to blindly wrapping the new evaluator in the existing thread-local guard. Keeping the existing symbolic-variable bound is necessary while `apply` and quantification recurse. Schema application itself should still lose its old recursive-depth heuristic once the frame conversion is complete.

## Evidence from the actual existing helper

`probe.rs` imports the unchanged production `internal-json/src/work.rs`. Rust 1.99 debug/release and minimum Rust 1.88 release on macOS arm64 all produce the same results, retained in the three logs:

| Control | Existing helper result |
|---|---|
| Outer 2 KiB; eight recursive frames containing 1 KiB each, deepest charge | charge false, outer deep true |
| Outer 2 KiB; inner 64 KiB; same recursive frames | charge true, both guards report deep false |
| Outer 64 KiB; inner 2 KiB exhausted but result ignored; outer charge afterward | inner reports deep, outer charge true and outer deep false |
| Outer 2 KiB; one 32 KiB helper entirely between charges | later charge true, deep false |

These are **limitations of the proposed reuse**, not evidence that an existing SDK production path currently invokes those nested patterns. `guard` overwrites the old origin/cap and restores its previous deep flag; unlike work `scope`, it does not aggregate nested stack exhaustion. `pass.rs` also deliberately restores its own pass-local reason, so it is not a shared sticky stack ledger either.

The large-helper example is fundamental: sampling observes only addresses where a check executes. A helper can reserve and release a larger frame between samples. Checking after it returns cannot recover its peak. Both debug and release probes use `black_box` to retain the arrays; no unsafe memory operation or fabricated depth estimate is involved.

## Minimal implementation contract

1. Capture a **live entry marker** in a small `#[inline(never)]` synchronous wrapper outside the actual scheduler. Keep it live across the inner call, for example with `black_box` before and after. Otherwise a marker created inside the already-large scheduler frame omits that frame's own entry allocation. Prefer a separate non-inlined sampler with an address-taken local. Pointer-to-integer conversion and numerical `abs_diff` need no unsafe dereference; retaining the number is not a reference or a promise that arbitrary stack addresses remain usable.
2. Hold only the integer origin, allowance and sticky exhausted flag in call-local guard state. Nothing goes into Plan, prepared artifacts or a cross-call cache. Do not resume that guard on another thread, fiber or asynchronously suspended continuation. Public evaluation is synchronous; preserve that assumption. `None` can support isolated Graph tests, but every production original evaluation must install its guard.
3. Check on every Graph work/tick route, including recursive `apply` and quantifier entry. Add deliberate boundaries before/after predicate/compiler/helper calls that can do material work without Graph ticks. A charge-only path and final conclusion must not evade an already observed exhaustion. Check before starting a retry, and **never** reset the origin/flag on retry. Preserve current cancellation semantics; stack refusal is `LimitExceeded`, never an effect, `AbortAttempt`, UnsupportedCapability or Undefined.
4. Avoid copying independently mutable exhausted flags between nested callers. If an internal evaluation/guard can nest inside one guarded call, either pass the same guard (with any child tighter allowance checked too), or implement explicit nesting that checks every enclosing address budget and propagates observed exhaustion according to the outer attempt's failure policy. A looser inner guard must not replace the outer limit. If such nesting is structurally impossible in the current path, document and test that premise instead of advertising a generic composable TLS guard.
5. `abs_diff` removes an assumed downward-growth direction but is still a measurement of two selected local addresses, not an ABI-defined stack-pointer reading. Compile layout, inlining, register allocation and local placement affect it. Count zero/tiny budgets explicitly; a zero budget cannot acquire an accidental success because two optimized markers happen to coincide. Keep actual host-capacity preconditions distinct: increasing `stack_bytes` cannot grow a thread or engine stack.

A helper wrapper that takes a local marker after its own prologue also cannot prevent that prologue's allocation. The mechanism is useful for the already-bounded symbolic recursion and as a check against unexpected growth; it is not a replacement for knowing the largest unchecked step.

## Fixed floor and required reserve

Do **not** introduce an unsupported constant now merely to make low-stack tests return a particular diagnostic. The review can approve the monitoring mechanism while leaving the fixed-stack certificate open. Before claiming the full stack gate, record two separate quantities for each supported build/host class:

* the fixed maximum live stack from the guarded entry through scheduler/predicate/error/unwind paths;
* the maximum additional unchecked stack between a successful guard and the next guard, including a recursive Graph prologue, quantification calling apply with its own frames still live, pattern/compiler helpers, exact arithmetic, allocation/error handling and teardown.

Use emitted stack/frame information and a call-path audit to justify these quantities, then validate them with small-thread-stack and instrumented peak controls. Measurements alone do not prove an arbitrary multiplier. A genuinely protective threshold needs room for the justified maximum unchecked step and caller/entry requirements; simply refusing **after** sampled use exceeds the whole configured allowance admits an overshoot by design. Until that margin is justified, describe this as observed stack-growth refusal and retain an honest open qualification item. No particular numerical floor or new symbolic-depth coefficient is approved here.

Application-provided `WorkControl::check` can call arbitrary user code; the SDK cannot certify that callback's stack consumption. State that boundary, and test benign synchronous reentrancy/cancellation separately. Sampling on either side does not transform arbitrary callbacks into a bounded SDK helper.

## WebAssembly qualification

A pointer-derived address in Wasm observes the compiler's addressable linear-memory stack, not a specified measurement of every engine activation/operand frame. WebAssembly specifies its implicit values/control/call stack separately from memories; function call frames hold locals. Therefore the inference is that linear address deltas alone cannot certify the engine's call-stack headroom. See the primary [WebAssembly runtime stack definition](https://webassembly.github.io/spec/core/exec/runtime.html#stack). Keep the explicit symbolic recursion bound and actual supported Wasm-engine tests; do not transfer the native probe result into a Wasm safety claim.

## Acceptance controls for the implementation

* Fresh debug/release and minimum/current Rust: repeated calls, tiny/exact boundary budgets, an observed overflow remains sticky after unwinding to shallow code, cancellation before/after overflow, failure followed by a clean independent retry.
* Active `apply` recursion, quantifier recursion **plus** nested apply, and formulas near the retained variable bound. Test actual small thread stacks separately from a small configured number on the ordinary host stack.
* A helper that allocates before its first check, and a helper wholly between checks, must be explicitly covered by the fixed-path reserve or remain an acknowledged unsupported safety claim; do not make the current probe falsely pass by dropping its allocation.
* Nested outer-smaller/inner-smaller guards, ignored inner result, unwind, and concurrent independent calls. No allowance or sticky state leaks into a later call.
* Actual Wasm Node and browser runs: observed-address behavior, bounded symbolic recursion and trapping/no-trapping behavior at the selected engine stack capacities. No universal call-stack proof from a native address probe.
* All existing required outcomes, recursive annotation controls, 20k/20,001/50k jobs and deep controls at unchanged defaults. The guard must not silently replace dropped recursive schema-depth checks with a new unexplained logical depth restriction.

This is an actionable design choice, not a request to preserve the outgoing compiler's historical stack coefficients. The original frame implementation and full physical-stack gate still require independent review.
