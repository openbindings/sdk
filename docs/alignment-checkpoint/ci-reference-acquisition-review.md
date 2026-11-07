# Independent acquisition/reference tooling review

**PASS, bounded to the six files in `source-hashes.json` at SDK `46186957c2a4494405b92d6e1c5efdd8245347ea`.** No production edits. No changed value oracle, disabled integrity gate, package upgrade or retrieval permission was found. One non-blocking stale section citation remains in a test comment, below.

## Lock and acquisition checks

Independent parsing of the old and current standalone lockfiles confirms all **197 package identities and all other package fields unchanged**. Exactly two dependency edges were added: `jsonschema -> idna`, `openbindings-json-schema-evaluator -> regex-automata`. Their selected identities match the root lock. Six unchanged retrieval-only registry identities are not in the root lock; that is not a change in this repair or grounds to alter them.

Rebuilt `tools/no-io` with the recorded Rust 1.99 environment, `--locked --offline`; build passed without changing either lock. The resulting binary SHA-256 is `67c0b089f2c77da461767ff3c39e310ffc0f6c5473fc5a0a9a71c1bcb6f131c3`. Independently reran the complete real HTTP/FIFO verifier. Both raw-library positive controls fetched and evaluated the false resource. Both SDK runs made zero observed fetches, and each passed five typed semantic/boundary checks. Report and full JSON observations are under `no-io-authorized/`.

The opaque custom-profile input is preserved. Its earlier `UnsupportedDialect` expectation was obsolete: the current conservative namespace certificate refuses external selection with exact `UnsupportedCapability(custom-profile-namespace)` before treating a selected missing carrier as established. This remains an SDK capability boundary, not InvalidExternalSchema or Undefined. Requiring that exact cause preserves a meaningful control; accepting any preparation failure would not.

The additional supplied known-profile case is materially stronger: the selected original meta-schema declares standard 2020-12 interpretation and explicit core/applicator/validation vocabulary, then requires the trap URI via `$ref`. Its actual admission must refuse `Resolution(ResourceUnavailable { uri })` with the exact trap URI. This exercises meta admission rather than simply re-labeling the opaque-profile case. Ordinary missing evaluation content still yields per-value ResourceUnavailable; kind/data remain uninterpreted; explicitly supplied false remains Fails. No I/O is authorized by any of these SDK paths even when dependency retrieval features are unified.

The verifier still requires both positive hits and zero SDK hits, records the executable hash, and states that this is an active finite witness rather than a universal proof of no I/O. Its scope wording now accurately includes custom-meta admission and `data`. No assertion was weakened. Native-loopback/FIFO results are not a Windows run or an all-host qualification.

## Generated reference checks

The rewrite is confined to three known README-relative spellings on the default-evaluator index. Their target files must exist. Destinations include the exact candidate Git commit and repository-relative source path. Other pages, unknown relative links, path escapes and unsupported schemes remain subject to the ordinary checker. All current public Rust roles are included in both generated landing indexes.

Independently ran all **four** production reference tests and **nine** additional controls: all three spellings/exact commit, idempotence, unknown link, directory escape, file URL, each of the three missing source files, and unchanged same-spelling link on an unrelated page (grouped as recorded by `independent.py`). Negative controls fail for the intended reason; none are suppressed by source-link rewriting.

Independently verified the already-built **clean** candidate `target/reference-candidate-4618695`: all **804** manifest file hashes, all recorded source hashes, exact commit/clean status, all current role index entries, exact source-link destinations, and all **8,531** checked local anchor targets. This rechecks the resulting artifact; it does not claim an independent second full rustdoc build. External URL availability and anchor fragment resolution are explicitly outside the local-link check, unchanged by this patch. A dirty preview remains labeled as such and uses its committed source base plus the manifest's actual source hashes; it is not a clean candidate.

The first-use example diff changes only local names and user-facing schema wording. The preparation/evaluation calls, values, branches and error handling are unchanged.

## Minor note and evidence limits

P3, documentation-only: `tools/no-io/src/main.rs` still cites “§5.2” for an external foreign dialect. In pinned `c1419c9` the governing schema section is **§5.5**. Correcting that comment would improve attribution but does not affect this PASS.

`initial-environment-results.json` preserves separate review-environment/driver errors: the first test shell lacked Cargo; the first HTTP listener lacked permission; an extra private lock assertion wrongly required every unchanged retrieval-only dependency to be in the root lock. Corrected runs are retained separately, without treating those failures as SDK defects. Source files were clean and unchanged during the final checks.

Hosted CI, future execution-frame implementation, whole SDK alignment, matched performance and the still-open ordinary compiler allocation/retirement gate are not closed by this tooling receipt.
