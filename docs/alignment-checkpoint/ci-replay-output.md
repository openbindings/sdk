# Replay receipt output investigation

Windows runs at 51e1dc7 and 5d7e8d4 reached the debug replay example and showed no
further replay output before cancellation. At 5d7e8d4 the example started at
06:35:19 UTC and cleanup started at 06:49:14 UTC. Although its elapsed job time was
approximately 30 minutes, the check annotation identifies supersession by the
higher-priority CI-90 request, not a timeout. Do not classify that run as a
completed failure of the replay itself. The earlier 51e1dc7 annotation does
explicitly report the 30-minute maximum execution time. Both annotations are
preserved beside this receipt.
Linux/macOS, browser, package, minimum Rust, Node floors, Wasm reproducibility,
advisories and performance passed at that implementation checkpoint. The Windows
qualification remains open; this is not a whole-host qualification claim.

The example previously sent its complete 3.2 MB receipt to the host console as a
single line. The portable verifier now sends that stdout directly to
`CARGO_TARGET_DIR/qualification/evaluator-replay.json`, parses it and prints the
observation count and path. Native CI preserves the complete file as an artifact.
Stderr phase markers distinguish value execution, admission evidence, serialization
and writing. These changes investigate a possible console/log-transport problem;
they do not establish that it caused the Windows stall.

The example still executes every original case, builds every admission record,
serializes the full receipt and asserts report success. It remains a debug run;
no test, expected result, evaluator limit or CI timeout was removed or enlarged.
Local replay passes. Its parsed output is exactly equal to the full-verifier
receipt from 5d7e8d4: 486 document admissions, 1,546 observations and zero
configuration failures. The complete output is 3,214,367 bytes, SHA-256
`ca338f4a3dcf87cafd7701516a8d3fc14de64396af73867f93e10fc436a68aa4`.
Formatting and strict example lint pass. The complete portable verifier passes
on clean pushed commit `24ad29467ef1f2bcb4bfab94d43b7bfa22b92128`; its retained
receipt is exactly equal to the focused local receipt above. See
the coordinator's `full-verify-24ad294.log` and the log hash in `results.json`.
The hosted Windows result is recorded below.

## Captured Windows result at 24ad294

The Windows receipt is complete and parsed-identical to the passing local receipt.
Value execution began at 07:11:08.838 UTC and the complete receipt was checked at
07:11:12.369 UTC. The job then passed Rust prose/examples, source-pull/invocation,
resource/diagnostic verifiers and the three outgoing evaluator stack targets. At
07:24:51 it started compiling the original-frame release test binary; the job-wide
30-minute cap cancelled that compilation at 07:26:12. This run establishes that
replay itself and writing the captured receipt complete promptly. It does not
prove the precise cause of the earlier uninstrumented console-output stalls.

This checkpoint changes only the Windows CI job ceiling from 30 to 45 minutes,
to allow the complete cold-build verifier to finish. Linux/macOS keep 30 minutes.
No SDK allowance, assertion, corpus case, matched performance threshold, build mode
or required check is weakened. The full Windows gate remains pending until a
completed run; this timeout extension is infrastructure capacity, not a passing
validation result. Raw log, annotation and full artifact are preserved in the
coordinator's `receipts/2026-11-02-preparation/ci-replay-output/` directory.

## Windows follow-up at 3cf0f6e

The 45-minute ceiling exposed a real test failure rather than another timeout.
The outgoing `walk_stack` release subprocess aborted with `0xc00000fd`
(`STATUS_STACK_OVERFLOW`) at 08:01:20 UTC on November 4. The preceding pattern and
evaluation stack tests passed. This test did pass at 24ad294; 3cf0f6e changes only
CI capacity and its receipt, so these observations do not establish a production
source regression or a precise root cause. The test does not emit individual
walk/depth progress. The complete raw log is retained as `windows-3cf0f6e.log`.

The whole Windows gate is failed and remains open. No admitted-depth expectation,
SDK resource allowance or assertion is waived. The replacement original-node
engine is still being qualified independently; retirement of the outgoing compiler
must be followed by the final default-engine checks on Windows. The next pushed
checkpoint, `67dfdeb`, removes obsolete regex-format identifier initialization
accounting and has its own bounded reviewed receipt; it does not claim to fix this
walk overflow.
