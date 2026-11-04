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
Formatting and strict example lint pass. Clean-commit portable verification and
the next hosted Windows run remain necessary before calling this a correction.
