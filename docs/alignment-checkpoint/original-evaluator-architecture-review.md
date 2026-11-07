# Ordinary evaluator architecture alternatives — independent feasibility review

**Decision: pursue one evaluator over original schema nodes, with a prepared concrete execution path and the existing bounded semantic analysis. Do not switch every evaluation to the present `Plan` unchanged. Keep the ordinary compiler allocation gate open until either the compiler is removed from production preparation or every remaining entry has an audited certificate.**

This is a read-only architecture experiment. It approves no production implementation, allocation coefficient, performance exception, or semantic oracle change. No SDK source was edited. The isolated bridge bypasses `compiled_environment` and constructs the existing `semantic::Plan` under the same `PreparationLimits` attempt, original environment, limits and controls. The companion's normal public `DefaultValidator` then evaluates it. The other lane follows the current production preparation method. Both use the same copied source, compiler, instrument and locked dependencies. Original and modified source identities, bridge code and binary identities are retained alongside this report.

## What the experiment establishes

Both paths pass all **1,546 current required kit cases**, including the kit's explanation-consistency checks, with zero configuration failures. This is a strong finite feasibility result, not a proof of full JSON Schema support. The semantic path already owns the current required foreign/custom profiles, exact numbers/UTF-16 values, original reference identity, dynamic dispatch, unavailable/undefined distinction and recursive proof machinery. No new primitive backend is needed to achieve those finite answers.

The frozen evaluator-bench requests and expectations are preserved verbatim in `frozen-workloads.json`. Observation-only requests disable the benchmark's assertion solely so the report can retain failures and compare them; they do not change schemas, values, resources or limits. Failed assertions in Explain are retained as errors. No refusal is counted as successful throughput.

Direct replacement loses these already supported jobs:

| Unchanged job | Current | Existing Plan only | Necessary change |
| --- | --- | --- | --- |
| 20,000 integer items (`array`) | Satisfies | analysis-memory limit | Avoid persistent per-occurrence proof states and unneeded item annotations |
| 50,000 integer properties (`large-object`) | Satisfies | analysis-memory limit | Demand-driven annotations, compact concrete traversal |
| 20,001 items with final invalid item (`late-invalid`) | Fails | analysis-memory limit | Same; preserve the late failing witness |
| Recursive array depth 32 (`deep`) | Satisfies | analysis-depth limit | Bounded iterative traversal for ordinary recursive values |
| 1,000 / 5,000 repeated long references | Satisfies | preparation-work limit | Single original-node syntax admission and shared bounded static query preparation |
| `source-diagnostics` | Fails with original maximum location | Fails with no problem | Bounded original-node diagnostic witness |
| `explain-one`, `explain-many` | Fails with one located problem | Harness rejects empty problem list | Preserve first-problem contract without inventing diagnostics |

An additional independently specified control, `contains` integer over 4,096 integers, must satisfy; current production does, but Plan reaches its memory limit. Its `contains` loop currently rebuilds a count-distribution vector of length 1 through n+1 even when every result is concrete. The 128/1,024/4,096 controls are separate from the frozen workloads.

Retained evaluation is materially slower, not merely a changed budget cutoff. Uninstrumented exploratory paired samples are in `paired-timings.json`; allocator observations are separate. `summary.json` records exact medians/ratios. Representative slowdowns are tens of times for scalar/object/reference/oneOf work and several times for large literal/uniqueness/pattern jobs. Preparation generally improves: warm context preparation avoids projection, conversion and backend compilation. `construction-observations.json` retains both successful status and requested allocation observations. No end-to-end gain is claimed by substituting an earlier preparation/evaluation refusal.

These are three exploratory adjacent pairs, not the benchmark's two batches of twenty randomized calibrated pairs. They were not taken in the final quiet campaign and cannot close a timing gate. The failures and large cost differences are sufficient to reject an immediate switch; exact policy thresholds remain unchanged. The target footprint stayed below 1 GiB.

## Source causes and the smallest coherent replacement

### 1. Prepare original instructions once; retain original identity

Extend `Plan`/`preparation::check_from`, rather than introduce another public evaluator abstraction. A private prepared-node record can own or index the node's effective profile, activated operands, original diagnostic coordinates, children and static resolution result. It must retain source-record/node/resource identity and exact source values, never normalize user schemas into a vendor resource. Static lookup already happens during preparation and is currently discarded; retain that result once. A dynamic target still dispatches against the current effective scope. Cache a typed malformed/missing/unsupported result as data, not as a blanket preparation refusal that erases decisive branches.

Prepare numeric bounds, count bounds, exact constant/enum representations and immutable pattern programs under the shared construction attempt. Repeated calls must not reparse the same numeric operand or recompile a PikeVM. A mutable VM search cache belongs to bounded per-call scratch, independently of the immutable compiled pattern; do not smuggle a cancellation control or an unaccounted mutable global cache into a validator. Publication happens only after the whole construction/meta-admission obligation succeeds. Failed/cancelled attempts and lower warm-cache budgets remain refused correctly.

Keep cache keys contextual. Two identical `$ref` strings from different original carriers/bases are not interchangeable; equal original JSON node indexes in distinct arenas are not interchangeable. The existing original query, exact decimal/UTF-16 semantics, profile activation and private-undefined classification stay authoritative.

### 2. Concrete execution is the ordinary case; formula reasoning is a promotion

Use the same original instruction graph and assertion helpers for a compact concrete result and for an unknown/formula result. Ordinary scalar and container validation needs no effect identity, no BDD variable, no complete memo entry per distinct primitive instance, and no clone of an empty `State`. Currently `schema` always assembles Visit/context bookkeeping and memoizes copied States, while `body` repeatedly reads/decodes operands. `Graph` does simplify TRUE/FALSE algebra already, but that does not remove the surrounding allocations.

Preserve active occurrence identity for recursion, and retain certified memo where it actually prevents repeated shared-reference work. A tree traversal may discard a child's concrete local state after its parent consumes it; a same-instance reference diamond needs bounded memoization. Do not remove memoization indiscriminately or replace exact effects with per-visit independent booleans.

On encountering a real local undefined effect, unknown external content or capability limitation, promote to the existing coherent formula representation. Promotion must preserve prior concrete annotations and evidence. Alternatively restart through the existing semantic kernel with the *same cumulative work/allocation ledger*; no reset and no accepting the first failed attempt as an Undefined witness. Keep existing recursive certification, quantifier order, pending-fact isolation and no-progress rules. There should be one meaning for applicators, with optimized concrete cases, rather than two independently evolving evaluators.

### 3. Collect only annotations an actual consumer can need

No `unevaluated*` consumer can observe root annotations that the API does not expose. Prepared demand can propagate through same-instance reference/applicator edges; new instance positions establish their own local demand. With no demand, omit property-name UTF-16 copies, item maps and their memo clones. When demanded and concrete, use bounded original member/item coordinates or bitsets, rather than a tree map and owned key for every member. Translate to formula-valued coordinates only where semantic dependence requires it.

Unknown inventory/dynamic edges must conservatively retain demand. Propagation must preserve successful-branch annotation rules for anyOf/allOf/oneOf, conditionals, dependencies and unevaluated consumers; `not`/propertyNames discard child annotations. Never conclude absent support merely because an unresolved branch was skipped. Existing coordinate-refinement tests are mandatory controls, including `valid=TRUE` with annotation depending on an unknown, exact lone-surrogate names, and conditional pattern-produced annotation correlation.

### 4. Count concrete `contains` matches directly

When a child answer is concrete, advance a scalar count and any demanded evaluated-item bit. Test exact min/max bounds without constructing the n-wide distribution. For uncertain matches, retain coherent formulas; a bounded saturated count-state representation up to the thresholds of interest is possible, but must keep count correlations and 2020 versus 2019 annotation behavior. Do not infer Undefined from a count interval. Early success may stop only when no upper bound or needed annotation can still change the answer; early failure is safe only with the corresponding proof. The current additional controls require S at 128/1,024/4,096 and should scale linearly in the concrete case under unchanged limits.

### 5. Iterative frames, diagnostics, and retained owners are required parts

Current semantic depth is `stack_bytes / 4096`, versus the compiler path's much smaller estimated frame cost; merely increasing the default or dividing by an unmeasured constant is not a fix. Explicit bounded frames remove native recursion from ordinary nested-instance traversal and charge actual retained frame structure. Preserve cancellation at entry, during loops, after calls and before return. The recursive semantic fallback can retain its separately bounded depth policy, but must not preempt a concrete proof the new ordinary executor can safely establish.

Honor `Detail::Explain` using original predicate/instance coordinates. A first bounded failure witness is enough for existing workloads; carry a compact witness through concrete evaluation or perform a separately bounded explanatory pass after the result. Its exhaustion must not retract Fails/Satisfies. Keep safe messages, original-source provenance, exact pointers, capped path materialization and no fabricated target origin. Do not solve the missing diagnostic by retaining the whole vendor projection.

The retention probe shows current complete compiler: 0 retained original arenas; Plan: 2 selected document/resource arenas; both release to baseline after the validator drops. An unrelated discarded set is released in both lanes. Thus there is no demonstrated leak of another dropped external set. However, retaining the full public environment keeps its entire document/resource pool and potentially large unused content. Prefer a private owner of the necessary original inventory/source closure, with no borrowed request/control and no requirement for retaining a public document cache owner. Zero original arenas is an implementation-specific old assertion; exact source ownership, unrelated-snapshot release, bounded retained bytes and final cleanup remain real requirements. Any changed retained-footprint expectation must be reviewed explicitly, not silently dropped.

### 6. Fix repeated syntax accounting by changing work, not its declared cost

`dialect_validity_bounded` reserves/charges 64 times the *entire original node text* for every supplied node, while its fixed meta-schema cuts child schema recursion. Long repeated references therefore exhaust the preparation ledger even though the ordinary compiler path currently succeeds without fully accounting its own construction. Do not lower this multiplier merely to recover the fixture. Prepare/check each original structural position once with a local syntax cost tied to actual keyword/operand traversal, and separately charge literal/identifier bytes which really are inspected or copied. Preserve full syntax admission of unused definitions/contentSchema/orphan conditionals and custom-meta dependencies. A shared bounded local validator/inventory pass is simpler to audit than revalidating the root subtree's text at every node. Warm cache reuse must still enforce the declared retained footprint and per-call work limits.

## Why not keep a tiny jsonschema lane?

It is possible, but not currently the smallest audited design. The locked public backend still performs its full meta-schema construction/admission path before compiling even a narrow ordinary schema. The previous backend schedule documents the hidden native cold cache and Wasm rebuild, registry/cache/hash-table/layout/error-path obligations. A whitelist of type/number/properties alone does not eliminate them. There is no publicly accessible `without_schema_validation` option to remove that obligation. A custom fork would introduce another maintained boundary and authority split.

The concrete parts of the existing native original evaluator already cover the small lane's useful work. Optimize those instead. Retaining the old compiler temporarily is a migration choice with an **open budget gate**, not a qualified escape hatch. Any eventual retained backend lane must be selected before projection/backend entry, have independently justified limits for every construction site, and be unnecessary for required semantic answers. Do not attempt backend construction first and call a semantic fallback only after an unbounded expansion or exhausted attempt.

## Executable sequence and completion gates

1. Freeze the current required observations, original workload bytes, diagnostic controls, registry identities and baseline binaries. Keep this Plan-only experiment as the adverse control.
2. Add immutable original-node preparation artifacts and minimal private ownership. Reuse existing native primitives/query semantics. Qualify aggregate construction reservations, cold/warm cache behavior and cancellation before using a prepared artifact.
3. Add concrete scalar/container execution, demanded compact annotations, concrete contains counting and reference sharing. Promote into existing formula/cycle logic only when required. Restore the failing ordinary workloads under current budgets.
4. Add iterative nested-instance frames and bounded first-problem explanations; preserve the existing source/instance locations and output contract. Re-run ownership/release tests with the chosen original retention model and measure unused-pool retention explicitly.
5. Force the new engine for all 1,546 kit cases, 505 core cases, 2,348 current auxiliary obligations, original recursive/coordinate matrices, exact primitive/pattern/format controls, current Node/native parity, interruption/overflow/warm-cache controls and standard/custom meta admission. Passing the 1,546 alone is not the whole gate.
6. Run the existing unchanged evaluator performance protocol with separate allocation/timing binaries and a quiet matched-toolchain baseline; preserve outcome/heap/count thresholds, no accepted residuals silently added. Include the existing Wasm memory caps, runtime tiers, diagnostic/owner controls and native/MSRV/Wasm execution. Evaluate both preparation wins and retained-call costs, not a geometric average that masks a failed job.
7. Only then remove ordinary projection/compiler/stand-in/bounds plumbing and its obsolete API documentation. Retain fixed normative schema machinery only where separately needed and separately bounded; this proposal does not claim the whole jsonschema package can immediately disappear from core's dependency graph.

The receipt's `implementation-controls.json` provides concrete restoration controls and invariants. None authorize weaker semantics, larger default limits or new accepted performance residuals.
