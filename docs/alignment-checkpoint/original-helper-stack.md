# Original-engine helper stack witnesses

Source checkpoint: `67dfdebbd515beaf5297f410db007d5b1ecd1920`. This finite lane
covers setup, original preparation, Evaluate, Explain and destruction on the
requested thread or Wasm linear stack, while 16 KiB of explicit caller padding
remains live. No default-engine switch or SDK resource-limit change is made.

The 323 controls comprise all 62 independently reviewed numeric cases, 245
required-format cases, and 16 additional large arithmetic cases. The latter use
512/2,048/4,096-digit numerators and 256/1,024/2,048/4,096-digit divisors, both odd
and even divisors, and exponent differences 0 or 40,000. Their expected answers
come from independent Python integer modular arithmetic. Existing numeric cases
include the retained 20,000-value and 50,000-property workloads. Every admitted
value's Evaluate and Explain classification matches its expected classification;
preparation/refusal cases retain their distinct expected outcomes.

| Host | Cases | Failures | Check-site sampled maximum |
| --- | ---: | ---: | ---: |
| Rust 1.99, macOS arm64, 128/256 KiB threads | 646 | 0 | 10,953 bytes |
| Rust 1.88, macOS arm64, 128/256 KiB threads | 646 | 0 | 10,009 bytes |
| Wasm, Node 22.23 | 969 | 0 | 5,413 linear bytes |
| Same Wasm, Node V8 engine stack 256 KiB | 969 | 0 | 5,413 linear bytes |
| Same Wasm, Chromium 156.0.8078.4 | 969 | 0 | 5,413 linear bytes |
| Same Wasm, WebKit 27.2 | 969 | 0 | 5,413 linear bytes |

Each Wasm run includes separately linked 64/128/256 KiB linear stacks. Native
cases run in separate processes. Wasm cases get fresh instances and normal
wasm-bindgen 0.2.129 host imports, including real cryptographic randomness for
document identity. Chromium uses normal asynchronous initialization and batches
of 32 cold cases on pages that close after each batch. No unlimited-stack or
unlimited-synchronous-compilation browser flag is enabled.

The coordinator receipt `receipts/2026-11-02-preparation/original-stack-qualification/helper-results.json` records input/source/binary/receipt identities. The
review-only bridge is appended to a copied evaluator and selects the original
Plan directly; the SDK's ordinary default still includes its outgoing compiler.
Address samples occur at cancellation checkpoints and are **not** maximum stack
measurements through opaque dependency calls. Successful small-stack completion
is separate evidence. Finite controls do not prove every helper path or arbitrary
caller-selected pattern depth. Compiler construction, unknown external frames,
native x86_64 hosts, combined budgets and independent final review remain open.

Harness failures are preserved and do not count as passing SDK observations:
raw Node modules refused the required randomness import; initial browser launches
omitted the installed-browser location; per-byte browser binary transfer exhausted
the Node driver heap. Normal generated glue and base64 transfer corrected those
setup problems. Chromium then refused synchronous initialization of the 9.4 MB
probe, and the asynchronous all-in-one-page run retained too many cold Wasm
instances (125 cases completed, then allocation failed). The final asynchronous
bounded-page run passes all cases. No SDK production correction was required.
