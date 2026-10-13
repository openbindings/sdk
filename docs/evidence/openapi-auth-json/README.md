# Authenticated JSON operations — performance-blocked checkpoint

The implementation is on `codex/sdk-openapi-auth-json`, based on
`abc96027691187ad11fe0394f1d1c9d73482e093`. The independent client remains pinned to
`fe09522a9b99a6c6b4918e3de0467b5a638afa5e`. **This slice is not qualified complete:**
two registered incremental-auth performance rows still fail after a bounded repair.
A subsequent [bounded measurement investigation](performance-investigation-report.md)
ended inconclusive: only 3/12 balanced null rows met its prospective 1 µs resolution
guard. No final qualification was allowed, so the original failures remain blocking.
No merge, publication, website, generic role, core, or binding specification change
is part of this checkpoint. Old binding drafts and their derived expectations were
excluded throughout.

## Implemented behavior

The same optional `OpenApi` provider now supports source-only inspection/synthesis
and per-attempt bearer/header-API-key invocation for the existing private 3.0 and
3.1 kinds. Primary source fixtures are OpenAPI 3.0.4 and 3.1.2. Existing binding and
operation invokers remain the public routes. Callers supply named credential strings,
a selected scheme-name set when needed, and explicit per-scheme destination-origin
grants in ordinary role context. No credentials enter operation input or generated
documents. There is no acquisition, refresh, store, retry, or new auth invoker.

The implementation handles conjunctions, alternatives, anonymous choices, effective
source overrides and supplied references. Missing context challenges before effects;
malformed/conflicting/out-of-scope context refuses before host entry. A 401/403 is an
ordinary kind status failure after one request. Statically unusable credential-header
combinations and unsupported mechanisms are honestly excluded from synthesis.

`context_bytes` bounds admitted encoded context; existing result/client/error budgets
bound metadata, headers and incrementally built challenges. The cache retains public
source analysis only. API keys remain opaque strings, including empty strings; the
client applies HTTP validity rules. This avoids inventing a shared nonempty-key rule.

See the [package guide](../../../crates/openbindings-openapi/README.md),
[frozen plan](plan.md), [case results](cases.json), [contract freeze](gate-a.md), and
[portability observations](portability.json). Source rules, possible future shared
meanings and implementation policies are explicitly separated. No experiment policy
is automatically promoted into a binding specification.

## Qualification and review

The final package run, including the test-only acceptance audit, passed affected
Rust tests, lint, extracted Cargo packages, TypeScript compilation, Node/Wasm, and
core dependency isolation. A separate minimal invocation consumer also passed.
Both native and Node/Wasm consumers run 1,000 mixed authenticated lifecycle cycles
per kind with logical-owner baselines and pending-host registration cleanup.
Loopback HTTP covers 200/401/403/302 with manufactured credentials and no redirects.

The final acceptance audit added explicit conjunction omissions, reordered selection,
retained terminal invocation, A/B origin grants, cross-kind/source isolation,
authenticated reference failures, API-key roles/conflicts, source preservation,
coverage, incomplete response delivery and exact 64 KiB context assertions. All added cases also passed native and Node/Wasm execution. Final
status is recorded in [summary.json](summary.json) and [the final receipt](qualification-final/receipt.json); earlier receipts are retained under
`qualification-before-perf-repair/` and `qualification-perf-candidate/`.

Independent reviews:

- Two GPT-6 Astra xhigh reviews: [initial findings and repair record](astra-reviews.json),
  [performance repair](astra-performance-repair.md), and [literal acceptance audit](astra-acceptance-audit.md).
- Bounded investigation: [Astra results](astra-investigation-results.md) and
  [Claude results](claude-investigation-results.md) independently confirm the inconclusive stop.
- Claude Opus 5.5 Extra: [initial review](claude-review.md), [contract/resource repair
  verification](claude-repair-review.md), and [performance-repair verification](claude-performance-repair.md).

The reviewers accepted the runtime repairs. Their code reviews are distinct from
coordinator-executed qualification, and neither model's acceptance overrides a failed
performance gate. Original failures and compiler/lint development diagnostics remain
in the evidence directory.

## Performance result

The [frozen policy](performance.md) is unchanged. Each run uses three fresh process
pairs and seven interleaved samples of at least 50 ms. Direct and adapter paths both
prepare requests, own attempt credentials, enforce client origins/headers, process
finite responses, convert results and destroy attempt state. The revised harness
checks exact wire values and one dispatch before timing. Both sides use identical
measurement source; [621 baseline files match the closed Git snapshot](closed-snapshot.json).

| Registered group | Revised result |
| --- | --- |
| Unsecured regression against the closed SDK | 24/24 pass |
| Incremental authenticated cost | 14/16 pass; 2 fail |
| Existing broad/source compatibility budgets, before auth-only optimization | 22/22 pass per edition |
| Authenticated source scaling and negative/selection paths | Diagnostic only; no equivalent direct coverage denominator asserted |

The failed rows are AND-authenticated 64 KiB POST through the operation facade, in
the first process pair:

| Edition | Measured incremental adapter cost | Allowed incremental cost | Excess |
| --- | ---: | ---: | ---: |
| 3.0.4 | 8.432 µs | 7.731 µs | 0.702 µs |
| 3.1.2 | 9.827 µs | 9.534 µs | 0.293 µs |

Later pairs pass, but the policy requires every pair to pass. The original run had
39/40 passing rows and remains failed. The [repair record](performance-repair.md)
explains the changed candidate and harness; the second result does not replace the
first. No external disturbance or threshold relaxation is asserted.

For scale, revised operation-facade medians across process medians are about 11.8–11.9
µs for bearer GET with 1 KiB output, 15.0–15.1 µs for API-key POST with 1 KiB input/output,
and 348 µs for the 64 KiB AND POST. Corresponding direct-client medians are about
7.6–7.7, 10.9, and 343–345 µs. These totals do not replace the differential gate.
Authenticated 512-operation synthesis is about 7.5 ms warm and 15.4–16.1 ms cold;
these source rows are diagnostic, not a comparative performance grade.

Raw results: [run 1](cpu-run1/summary.json), [run 2](cpu-run2/summary.json),
[3.0 broad checks](broad-304/summary.json), [3.1 broad checks](broad-312/summary.json),
and [auth source/error diagnostics](auth-diagnostics/summary.json). Every directory
retains raw samples and process output. No network latency, TLS, browser/Worker,
allocation-peak, RSS, JS heap, zeroization or competitive-grade claim follows.

## Reproduction and remaining work

With the locked toolchain, cached dependencies, Node, TypeScript and wasm-bindgen:

```sh
python3 tools/verify-openapi-json-path.py --archives --wasm
python3 tools/verify-openapi-invocation-only.py
python3 tools/measure-openapi-auth-json.py --baseline /path/to/closed/consumer --output /fresh/output
python3 tools/measure-openapi-auth-diagnostics.py --output /fresh/diagnostics
```

Build the closed snapshot from `git archive abc9602 Cargo.toml Cargo.lock crates vendor`.
Use the consumer from that commit, copy the current `examples/auth_measure.rs` into
it, and patch its local package dependencies to the extracted closed snapshot. Keep
the exact client revision. `closed-snapshot.json` records the manifest used here;
its absolute paths are local reproduction plumbing. The CPU runner checks identical
measurement sources and a fresh candidate qualification receipt. Output directories
must be new. Broad checks use `tools/measure-openapi-json-path.py --edition 3.0.4`
or `3.1.2` with a fresh `--output` directory.

The remaining blocker is **AJ20**. The [single bounded investigation](performance-investigation-report.md) has now
run and stopped inconclusive. It reduced observed residual variation but failed its
method-adequacy guard; no runtime repair or final qualification followed. The next
design decision concerns measurement resolution and the separation of sampling
variation from instance-placement effects. Preserve both failed runs and the current budget. A methodological or
budget change requires a recorded design decision and independent rationale before
a new qualification; repeated unchanged runs until green are not permitted.

Nonblocking follow-ups remain outside this slice: client/host treatment of API keys
named after connection/framing headers; the one-source cache's alternating-source
cost; production browser/Worker transport and package distribution. Broader auth,
additional media, source acquisition, schema translation and binding-standard
publication are not hidden requirements of this implementation.
