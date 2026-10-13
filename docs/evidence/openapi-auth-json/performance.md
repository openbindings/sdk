# Authentication qualification and resource budgets

These are prospective engineering gates for the authenticated JSON slice, frozen
before candidate implementation. They are not binding rules, a competitive SDK
grade, or a claim of HTTP latency parity. The earlier broad budgets remain historical
compatibility checks; they cannot substitute for the incremental gates here.

## Comparable measurements

Use release builds, a recorded host/toolchain, exact source/input hashes and fresh
extracted-package receipts. Retain all raw observations. Measure seven samples of
at least 50 ms in three fresh process pairs with alternating execution order.
Within an auth pair interleave the four matched conditions: direct anonymous,
direct authenticated, adapter anonymous, adapter authenticated. Use a programmable
host and identical finite response bytes; check the headers and effect counts before
timing. Measure binding and generic operation layers separately. Optional declared
schema evaluation uses the same evaluator/contracts on both compared paths.

Include request preparation, the client origin/header work, input conversion,
credential copies owned by the attempt, result conversion and destruction. Exclude
credential acquisition and initial caller construction equally. Do not compare an
adapter that parses/prepares values with a direct baseline that only reuses a fully
prepared request. Source loading is warm for warm rows and explicit for cold rows.
A direct-client reference consumer applies the same selected requirement and supplied
origin grants. It does not get to skip collision/origin checks.

Required auth workloads for each of 3.0.4 and 3.1.2, in both invocation layers:

| Workload | Successful JSON bytes | Credential material |
| --- | --- | --- |
| Bearer GET | 1 KiB response | 64-byte token |
| Header API-key POST | 1 KiB request and response | 64-byte key |
| Bearer + header API-key POST | 64 KiB request and response | Two 64-byte values |
| Bearer GET with larger context | 1 KiB response | 4 KiB token plus its grant |

Run additional diagnostic rows for missing-credential challenge, explicit wrong-origin
refusal and anonymous/security selection. Exercise 1/32/512 operations for authenticated
source analysis/synthesis, reusing the existing source-work gates with a matched direct
client/core authoring baseline and equivalent coverage work clearly accounted for.
If the baseline cannot produce equivalent outcome/retained data, report that comparison
as diagnostic instead of silently using a cheaper denominator.

## Frozen gates

1. **Unsecured-path regression:** reproduce the existing warm GET/POST 1 KiB/64 KiB
   workloads for both layers against the closed SDK commit `abc9602`, with the same
   client pin and host in the same run. For every process pair, candidate median
   `C <= 1.10 * B + 1,000 ns`. This tolerates small routing noise but detects a
   material new per-call penalty unrelated to auth. Historical timing tables are
   not a substitute for this controlled comparison.
2. **Incremental auth cost:** for each matched pair, let `Da`, `D0`, `Ca`, `C0` be
   direct-auth, direct-anonymous, candidate-auth and candidate-anonymous medians.
   Require `Ca - C0 <= max(0, Da - D0) + 5,000 ns + 2 * S ns`, where `S` is the exact
   encoded byte count of the supplied auth context (selector, credentials and grants).
   Also report the unadjusted differences, absolute medians, ratios and the fraction
   of the allowance consumed. The fixed allowance covers role/context bookkeeping;
   the linear allowance is an explicit provisional bound for extra scans/copies.
   This is an acceptance budget, not an inferred optimum or a cryptographic cost.
3. **Existing source/general overhead gates:** retain the earlier registered gates
   for unaffected work and relevant authenticated source work; retain failures and
   denominators. Do not claim that passing their relatively generous ceilings alone
   establishes good performance. No whole-network claim follows from mock timing.
4. **Deterministic structure:** repeated calls to the same source, with different
   credentials/context, perform no additional source preparation. No secret-bearing
   context or request enters the prepared-source cache. No cache lock extends across
   host I/O. The existing one-source cache and its alternating-source limitations
   remain documented; this slice does not redesign or requalify them as warm.

More than 15% baseline process spread is inconclusive. After documenting an actual
external disturbance, allow one complete repeat and retain both runs. Stable failures
require a focused explanation/repair, not repeated timing until green. A needed gate
change returns to a recorded design decision, retains the original result and gets
independent rationale; the executor cannot call a failed gate passed. No allocation-
peak, RSS, JS heap or browser-networking grade is inferred from these CPU results.

## Finite resources and credential lifetime

Before runtime edits, freeze a bound-to-cost table in the execution record:

- Proposed `Options.context_bytes = 64 * 1024`: logical encoded context admitted
  before auth scanning/copying. Define applicability explicitly so ignored context
  on an already-supported unsecured call is not accidentally a new semantic contract.
- `Options.result_bytes`: retained public security metadata and generated inventory/
  coverage, including strings and alternatives. Charge added retained fields.
- Client `document_bytes`, `nodes`, `reference_steps`, `descriptor_work` and `headers`:
  their actual documented metadata/traversal/input/header counts. No invented header-
  byte field; the proposed context bound covers raw credential input bytes separately.
- SDK `InvocationLimits.max_error_bytes`: final challenge/error encoding. Bound
  provider construction work too; checking only after an unbounded build is insufficient.
- Existing source/body/queue limits and cancellation checks continue to apply.

Do not require an internal map, counter design or cache algorithm. Do require a real
bound for each new cost and exact-limit/one-over cases. Bounds on logical values do
not bound a caller's preexisting parent JSON arena, process memory or secret lifetime
in caller-owned copies. No zeroization claim is made.

Run 1,000 mixed native and Node/Wasm cycles per kind with success, missing-context,
wrong-origin/refusal, HTTP failure, malformed delivery, and cancel/drop of a genuinely
pending local host. Measure owner and host-registration baselines after warm-up and
verify return after outputs/providers/jobs are dropped and work settles. Add explicit
host-entry and output counts to Node error paths. Review that cached analysis holds
only public source metadata; completed attempts must release their own credential/
request state. A successful remote body may itself contain arbitrary data, including
an echoed secret; preserving it is not a redaction failure.

Native loopback timings are supplementary. Use manufactured credentials only, with
no real third-party calls or account state. Programmable HTTPS-origin tests and plain
loopback HTTP do not qualify TLS, browser transport visibility, CORS or redirect-safe
credential handling in an arbitrary application-supplied host.
