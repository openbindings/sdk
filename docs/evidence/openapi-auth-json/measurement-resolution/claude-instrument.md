# Instrument review: measurement-resolution R/Q (Claude)

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Read-only. I ran no builds, timings or calibration, and did not read Astra's report.

**Reviewed against:** `measurement-resolution-design/synthesized-plan.md` (sha256 prefix `7fe8b446`).

**The source changed while I was reviewing it,** with barrier, fixture, target-dir and signal updates. The final snapshot reviewed is:

| File | sha256 prefix |
| --- | --- |
| `auth_resolution.rs` | `fd1e0c53` |
| `tools/auth_resolution.py` | `aab3f8a5` |
| `tools/run_auth_resolution.py` | `d3a573fa` |
| `tools/test_auth_resolution.py` | `b3e28291` |
| `resolve-…py` | `5ee037b1` |
| `qualify-…py` | `285f5579` |

**Executed (deterministic only):**
- `python3 -O -m unittest test_auth_resolution`: 4 tests pass.
- Read-only validation of the three readiness JSONL files as preflight rows.
- A synthetic validator cost check: a 7,413-block Q row parses and validates in about 0.02 s.

## Verdict

The instrument implements the synthesized plan faithfully on every point that bears on validity:
- identities;
- fixture sharing;
- schedule;
- block arithmetic;
- caps and checkpoints;
- R adequacy;
- the Q gates and envelope.

**One material reporting defect** must be fixed before freeze (F1). Four smaller items should go into the same allowed repair round. A defect I was about to report (an implicit target directory for the two builds) was fixed during the review. With F1 fixed, I see no blocker to freezing, provided `--prepare` is re-run on the final sources; `verify_extension` already enforces that.

## Material finding

**F1. A numerical Q failure is reported at top level as `inconclusive-stop`.**
- **Where:** in `summarize_q` (`auth_resolution.py`), every gate's status is correct (`fail`, `pass` or `inconclusive`), but the summary status is `performance-qualified-pending-independent-review` only when all gates pass, and **`inconclusive-stop` otherwise, including when some gate failed numerically**. That status goes into `provenance.json` and the exit path.
- **Why it matters:** the plan says "any original numerical gate failure remains a recorded failure" and "an invalid instrument does not erase an observed numerical failure". A reader of the top-level status would see a failed qualification described as inconclusive.
- **Fix:**
  - status `failed` if any gate is `fail`, whatever its validity;
  - otherwise `inconclusive-stop` if any gate is `inconclusive`;
  - otherwise the pending-review pass.

  Add a unit test for the failure case. The existing test checks only the gate-level `fail`.

## Fixed during review

**Build target directory.** `prepare()` built both consumers and copied `TARGET/release/examples/auth_resolution` while depending on `CARGO_TARGET_DIR` being set in the environment. Without it, cargo would have written each consumer's binary to its own default target. The runner would then have copied a stale or identical file as both "candidate" and "closed", and an anonymous-only readiness run cannot detect that. The current runner passes `--target-dir` explicitly.

The prepared binaries are distinct, and the "closed" hash equals the last build output. **Residual (cheap):** require the candidate and closed binary hashes to differ, and record cargo's reported executable path (`--message-format=json`) in the receipt extension.

## Smaller items (same repair round)

1. **Independent fixture checks.** `fixtures.json` is generated from the same executable's readiness output, so the frozen-hash check detects drift, not wrong fixtures. The independent checks are sound for the input and response strings, the source's servers, security and path/method, and the credentials, selection and grants. The per-object oracle also checks the exact wire target, body and auth headers, and that anonymous objects send no auth header. Two gaps remain:
   - the *configuration key* is not checked against the kind's exact security point;
   - `fixture_expected` carries an unused placeholder `kind` string.

   Check the key literally (`local.openapi-json-probe.r1.security` and `local.openapi30-json-probe.r1.security`) and remove the placeholder.
2. **The abort path drops a flushed partial row.** On SIGTERM the binary now finishes the current complete-call block, prints its partial row with an error and exits. But `invoke()` stops reading stdout as soon as it raises, so that row never reaches the raw file. The plan says a controlled abort flushes partial-row blocks. After `terminate()`, drain stdout to EOF (bounded by the existing 2 s wait) before closing.
3. **The consumer-file exclusion is unexplained.** `prepare()` skips `wasm-probe.ts`, as well as `Cargo.toml`, which is separately verified, when comparing extracted consumer files with the prior receipt. Record the reason in `receipt-extension.json`, or include the file.
4. **Missing tests:**
   - top-level `failed` status (F1);
   - frozen-fixture hash mismatch;
   - rejection of rows whose `error` is `record-cap`, `cycle-cap`, `wall-time-cap` or `interrupted`;
   - the new configuration-key check.

## Verified against the plan

- **Objects and identity.**
  - R constructs 2 cohorts × 8 objects; ids 0–3 share a common fixture, and 4–7 each get a separately allocated one.
  - Q constructs eight objects for candidate auth rows: real D0, Da, C0, Ca, plus null DA, DB, CA, CB. Closed and non-auth rows get four.
  - Construction ranks follow F, or its reverse for R.
  - Maps are correct: `R-alias [0,0,2,2]`, normal and swapped, `R-separate [4,5,6,7]`, and `Q-null [4,5,6,7] / [5,4,7,6]`.
  - The alias borrows the same retained closure, and the index is resolved before the timer.
  - Shared storage is real. `JsonValue` clones share an `Arc` arena and the body is `Arc<String>`, and both the Rust assertions and the validator check pointer equality and inequality.
- **Work equivalence.** The factory mirrors `auth_measure.rs` per-call work exactly: input conversion, owned credentials, preparation, host invocation, response decode and parse, and completion. The only addition is a post-preflight `Cell` check (`oracle.output`), identical on direct and adapter paths. Sources and providers stay dedicated to their object, and auth and anonymous sources never share a cache.
- **Blocks and schedule.**
  - Blocks are K = 64 for 64 KiB rows and 256 otherwise, every call timed.
  - Each configuration-sample starts with one untimed K-call prefix block on its final slot.
  - Sample size: whole 16-block cycles until each label reaches 800 ms (AND) or 200 ms. Each sample is `Σns / Σcalls`.
  - The cycle `0010203112132233`, its rotation and reversal formula, and the R and Q configuration and cohort visit orders all match the plan. The Rust test checks edges for every index.
- **Capacity.**
  - The Vec capacity is reserved at 16,384, and the validator requires that exact capacity, so any reallocation would be detected.
  - A pre-sample guard prevents overflow: at most 7 × (1 + 146 × 16) = 16,359 records.
  - The cycle cap is 146.
- **Validator.** It checks:
  - identities, cohort and construction order, fixture relations and object work modes;
  - per-block tags, cyclic predecessors, calls, object and fixture identity;
  - ordinals and global chronology, and that each configuration-sample is uninterrupted;
  - the macro schedule against `layout()`;
  - no extra cycle after the target, the duration target and exact sample arithmetic.

  It uses explicit errors that remain active under `-O`.
- **Barrier checkpoint.** Each row is written and `fsync`ed, then validated with the frozen fixtures, and only then acknowledged. The binary blocks on `next\n`, and validation is not concurrent with timing.
- **Caps.** The binary has its own monotonic deadline (remaining − 1 s), and the runner enforces 2,700 s including first launches and the settle.
- **Stop conditions.** There is a single-attempt directory marker and no retries. R finishes all processes before deciding. Q refuses to start without:
  - an adequate R that reproduces from raw data;
  - a matching freeze hash;
  - `review-acceptance.json` with summary and reviewer hashes.

  The freeze must be committed in HEAD and covers sources, the plan, evidence files, binaries and the compiler.
- **Arithmetic.**
  - R: 96 observations, 72 eligible, `|D|, |C|, |R| ≤ 1000`, and `U_R` taken over the eligible observations.
  - Q: the unsecured gate is `C ≤ 1.10B + 1000` on slot-2 medians, with closed-baseline spread. The auth gate is `max(0, D) + 5000 + 2S`, with S from the authenticated objects. It requires both nulls to be adequate, `slack > max(U_Q, U_R)` (U_R only on rows 4, 5, 16, 17, mapped correctly) and auth spread ≤ 15%. Numeric results and validity are kept separate per gate.
- **Feasibility.** Validator overhead is negligible: about 0.02 s for a 7.4k-block row. Checkpoints therefore do not eat into the 45-minute caps in any meaningful way.

## Known and accepted

- Closed Q rows run `Q-real` alone, while candidate auth rows interleave it with two nulls. This was disclosed in design review and is acceptable for the 10% + 1 µs regression gate.
- The macOS signal FFI is confined to the measurement example.
- The prior functional qualification is preserved, not rerun. The extension states that scope.

**Ready to freeze after F1 and a re-run of `--prepare` on the final sources.** I recommend folding items 1–4 into the same single repair round.

## Post-snapshot note

After I wrote the review above, `tools/run_auth_resolution.py` changed again (sha256 prefix `777935d2`). The other reviewed files are unchanged.

**Now addressed:** item 2. On abort, the runner calls `proc.communicate(timeout=2)` (killing the process if it does not exit) and writes and `fsync`s any remaining stdout tail. It also records a sha256 checkpoint for each accepted row before acknowledging it.

**Still open at these hashes:**
- F1, the top-level status;
- item 1, the configuration-key check and placeholder;
- item 3, the `wasm-probe.ts` exclusion reason;
- item 4, the tests;
- the residual binary-hash inequality and artifact-path check.

The final freeze should cite the exact hashes that the repair round produces.

## Final independent verification addendum

Verified read-only on 2026-10-13. No builds, timings or calibration, and Astra's report not read. My own execution was limited to:
- the 9 Python tests, normally and with `-O` (both OK);
- a manual replay of every `verify_extension` check except `rustc -vV`, because `rustc` is not on this reviewer's PATH. The receipt records `rustc 1.99.0 (b940084d7 2026-09-28)`, and `freeze` and `verify_freeze` re-check it on the executing host;
- revalidation of all 52 readiness rows (R 4, candidate Q 24, closed Q 24) against the frozen fixture table.

**Exact hashes verified (sha256):**

| Item | Hash |
| --- | --- |
| `auth_resolution.rs` | `959ec5a0db047b9975c1d5d4f20b79685975e795c32ba28983fc89c614f17974` |
| `tools/run_auth_resolution.py` | `02585376b402292c419c5b9c8eadbc13172be0cce7f05f5d5138a2afb6296f71` |
| `tools/auth_resolution.py` | `44867c81fcce03428e551585a13e8bbc0c7e7f4b374909abf1b082b213880625` |
| `tools/test_auth_resolution.py` | `20db2e9232feb732076a67329128d5c66d93f8f713d3a988c49d2f480211a167` |
| `resolve-openapi-auth-performance.py` | `5ee037b1b8f2f42060f365b1f01a92c1226ec90ae502dfcfb2cf29dc9137e865` |
| `qualify-openapi-auth-performance.py` | `285f55794b9e66f08b6f9a90b867a31c0778b265e4325a2c01f33ded0bea3d4d` |
| `synthesized-plan.md` | `7fe8b44641ca6f9cc9ad765110833abeaaa978d86b6f7708b2d3c1eb50e968cb` |
| `receipt-extension.json` | `ff6bd4ecdd698caaee9fbba9fdb34c7064cf555e2beade9374c569b0b20aec59` |
| `fixtures.json` (20 keys) | `485905adcd23147be4079a899e3fae21beb06fc92ae6539e3e61ec88e61d65ec` |
| `schedules.json` | `2716ed6aed7e249ecba88bec439bf8c757c441689118007091df5a9232172ea8` |
| Prior receipt (`qualification-investigation/receipt.json`) | `ccd8d47b8d0e40a9ae7f941289c531e70b63d209c0aef5860a4a18fcdd5c0347` |
| Closed snapshot | `98f269e1911989e7a2b1f8210cd1b49b005f1b8e5c1620d1e27d35a437d10076` |
| Binary, candidate | `e5f598e1e54eda1eaa7c8169ac688028aaa50b90834a126eaf957b971e9284d7` |
| Binary, closed | `5df55527f3ab3a083777f36618738a10defdc4eb8468915726d2a8c0c8006e6e` |

Both binaries were copied at 10:38 from cargo's JSON-reported executable, they are distinct, and both match the receipt. Their hashes equal the 10:32 builds. That is consistent with the final source edit being confined to `#[cfg(test)]` code: the 3 Rust tests, including the new controlled-stop test, ran in both test logs.

**Disposition of findings:**
- **F1 is fixed.** `summarize_q` now reports `failed` if any gate fails, otherwise the pending-review pass, otherwise `inconclusive-stop`. It is tested.
- **Item 1 is fixed.** There is a literal check that the configuration key is the kind's exact security point, and the placeholder is removed. A wrong-key corruption test was added. Exact fixture hashes for source, context, input and response are enforced from the frozen table in the barrier and in every re-read, with key coverage (20) checked in `prepare`. A hash-mismatch test was added.
- **Item 2 is fixed.** `stop_and_drain` terminates the process, collects its tail and `fsync`s it. The Rust side checks a cooperative SIGTERM/SIGINT flag only at block boundaries and emits the partial row. This is tested on both sides.
- **Item 3 is fixed.** The reasons for both consumer exclusions are recorded in the receipt.
- **Item 4 is fixed.** Tests now cover the top-level failure status, frozen-fixture mismatch, all four error codes, the wrong key, the barrier, corrupt and truncated rows, deadline and interrupt.
- **The binary residual is fixed.** Cargo `--message-format=json` supplies exactly one `auth_resolution` executable at the expected path; candidate and closed hashes must differ; and both are bound in the receipt and checked by `verify_extension`.
- **The barrier works as intended.** Each row is `fsync`ed, then validated with the frozen fixtures, then its sha256 checkpoint is recorded, and only then is `next` sent.
- **The closed consumer is pinned.** Its manifest must equal the snapshot, `src/lib.rs` must equal the base commit, its locked packages must equal the candidate's, and its sources must match the snapshot.
- **Linkage is complete.** The receipt binds the readiness hashes, the instrument hashes (checked unchanged across `prepare`), the compiler, command exits, packages and consumer inputs.
- **Acceptance markers are enforced.**
  - `freeze` requires `instrument-acceptance.json`, binding the receipt hash and each review's hash, and requires the marker below in both reviews.
  - Q requires an R acceptance binding the summary, provenance and raw hashes, with `FINAL R DISPOSITION: ADEQUATE` in both result reviews.

**Non-blocking notes:**
1. The marker check is a substring match. A future change could require it as a standalone line.
2. The checkpoints JSON is written without `fsync`. It is derived from the `fsync`ed raw rows, so nothing is lost.

No blocker remains.

FINAL INSTRUMENT DISPOSITION: ACCEPTED
