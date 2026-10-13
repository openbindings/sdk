# Independent performance-repair review

Reviewed on 2026-10-13 by the independent source/security review agent. This text
records the reviewer's own conclusions. It is a focused code and evidence review,
not a performance qualification result.

The review used the fresh authenticated-JSON plan and performance policy, retained
run-1 `summary.json` and `raw.json`, the current adapter security/invocation code,
the fresh consumer authentication cases, and the two measurement programs listed
below. No old OpenAPI binding draft, draft-derived expectations, or another
reviewer's conclusions were used. No benchmark was run during this review.

## Original result remains failed

Run 1 contains 39 passing gate rows and one failing row. The first process pair's
3.0.4 AND POST 64 KiB operation-layer incremental-auth calculation is correct:
8,109.215797705925 ns versus a 6,581.755244755244 ns ceiling, exceeding the ceiling
by 1,527.460552950681 ns. Later process pairs pass, but they do not erase this
failure under the registered policy. The retained samples show substantial
variation in small differences between approximately 350 microsecond totals;
that does not establish an external disturbance or authorize a gate adjustment.

## Runtime repair

No material correctness regression was found in the two narrow optimizations.

- `security.rs:64-105` now constructs the sorted, deduplicated choice inventory
  only when a challenge needs it. An explicit selector is still a duplicate-free
  set and matches the first profile alternative with exactly the same keys. This
  preserves reordered-name behavior and the old first-index result for duplicate
  equivalent alternatives. Without a selector, all alternatives must have the
  first alternative's key set before index zero is selected. The initial
  unsecured-profile return ensures this later indexing cannot see an empty
  security vector. Anonymous alternatives among authenticated alternatives still
  participate in explicit choice. The local `OnceCell` retains no per-attempt
  data after the synchronous call and adds no credential-bearing provider cache.
- `security.rs:167-187` now normalizes each supplied grant once in the adapter and
  accumulates a matching-origin Boolean. The loop continues after a match, and
  each later element still undergoes type and URI validation. Empty grants,
  nonmatching grants, and malformed grants continue to refuse. The existing
  client request preparation retains its final destination-origin check.
  `auth.rs:389` adds a matching-origin-then-malformed-origin refusal case to the
  shared consumer suite, whose negative-case assertions include no new host
  entry and no credential material in the returned error.

These changes remove identifiable redundant work. This review does not predict
their savings or claim that they repair the failed timing gate.

## Measurement repair

The missing pre-timing oracle is addressed in `auth_measure.rs`. Each condition
owns an independent `Oracle`. Its first warm-up verifies the exact target, body,
zero/one/two expected credential headers and their values, and exactly one client
host dispatch. The oracle is disabled before `sample` starts its clock. Expected
wire values are constructed from the benchmark fixture, not from the prepared
request. Subsequent loops retain inactive flag checks; the adapter also retains
its oracle ownership scaffolding. Their costs remain inside the measurement.

The reviewed direct and adapter conditions still perform corresponding body
conversion, client request preparation, client origin/header enforcement, finite
response decoding, output conversion, and destruction. Neither operation-layer
condition introduces optional schema evaluation. No prepared-request-only direct
baseline or omitted direct credential-origin enforcement was introduced. Fresh
candidate and closed-baseline receipts should identify the revised harness; old
receipts must remain associated with the original harness and result.

## Diagnostic harness

No egregious work or outcome mismatch was found in `auth_diagnostics.rs`.

- Warm source rows reuse a provider whose public source analysis was prepared;
  cold rows create and drop a fresh provider on each iteration. Inspection and
  synthesis-with-coverage are reported separately for 1, 32, and 512 operations.
- The initial synthesis checks the expected operation count. Source timings have
  no purported equivalent direct denominator and are explicitly marked
  `diagnostic`; they cannot be promoted into evidence that a comparative source
  performance gate passed.
- Missing-credential, wrong-origin, security-choice, and anonymous-choice rows
  check their expected terminal class and host-entry count. The helper's wire
  capture, clearing, assertions, and ownership costs are included, consistent
  with the diagnostic notes. These are binding-layer diagnostics, not additional
  operation-layer or network measurements.

## Disposition and limits

The narrow runtime and benchmark-procedure repairs are accepted on code review.
No material finding remains within this focused scope. The revised runtime,
consumer, and benchmark programs were not executed by this reviewer in this
pass. The earlier independent two-test challenge-budget check does not substitute
for current functional or performance qualification. Package, Node/Wasm, lifetime,
and revised CPU qualification remain separate work. The original run-1 failure
remains intact; no threshold, formula, workload, or pass rule was relaxed here.

Reviewed file SHA-256 values:

| File | SHA-256 |
| --- | --- |
| `crates/openbindings-openapi/src/security.rs` | `f64a8a79e9b7b21b4d3468ca41c0cbe92ff26fd724be71316bf3527564182c7c` |
| `examples/openapi-json-path-consumer/examples/auth_measure.rs` | `af7d571bc9a5bfd9999de44abfa7695297ce27bbd62bdec1dfb359b6a38e1ed4` |
| `examples/openapi-json-path-consumer/examples/auth_diagnostics.rs` | `311b35e12c4f32d2aef330b9ef6b29bb1c2f4b5e16806a2938f8f6d64451912a` |
| `examples/openapi-json-path-consumer/src/auth.rs` | `5a5c2257c843469696e77c977ff87e0b0b8caf531238af557b9ef1715432e031` |
| `/tmp/ob-auth-cpu-run1/summary.json` | `c5819ea084f6e628df3a131ccb70d5de50d3db221d5776b2cfb8fd9bf3f433a7` |
| `/tmp/ob-auth-cpu-run1/raw.json` | `ff8b71a2aa162dd0906a5c120ba9c50e57c3e5dcf2cf5333217cded54626abc5` |

## Addendum: revised run-2 disposition

I independently recomputed all 40 gate outcomes from the medians in
`/tmp/ob-auth-cpu-run2/summary.json` using the unchanged registered formulas.
The reported outcomes are correct: 38 pass and two fail. Both failures are the
first process pair's AND POST 64 KiB operation-layer incremental-auth rows:

| Edition | Measured increment (ns) | Ceiling (ns) | Excess (ns) |
| --- | ---: | ---: | ---: |
| 3.0.4 | 8,432.465175 | 7,730.757199 | 701.707976 |
| 3.1.2 | 9,826.571238 | 9,533.581493 | 292.989745 |

The later pairs pass, but the required per-pair gate remains unsatisfied. The
largest reported process spread across all rows is about 3.55%, below the
registered 15% inconclusive threshold. No external disturbance has been
demonstrated. These facts do not authorize discarding either failure or replacing
the registered decision with an aggregate or majority result.

Performance therefore remains a blocking acceptance item for qualified slice
completion. Retaining this failed checkpoint without more timing attempts or a
relaxed gate is the supported disposition. The code-review acceptance of the
narrow repairs remains separate from this unsuccessful qualification. This
addendum proposes no further optimization and reports no new product defect.
No benchmark was run by this reviewer.

Run-2 summary SHA-256:
`0504a8935431f2fee551dbe9220376bf6a585d34c7e0d5049a736c3573d94f12`.
