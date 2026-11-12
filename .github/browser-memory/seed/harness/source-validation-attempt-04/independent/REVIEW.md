# Closing measurement review

One bounded independent review of the completed harness repairs and preserved
executable/preflight evidence. This reviewer did not implement either repair.

| Item | Disposition | Material blockers in the reviewed repair/evidence |
| --- | --- | --- |
| A — browser scratch and STOP repair12 | Accept for root-owner decision on one next reviewed preparation/activation | None found. Active seals still bind the old source and must be prospectively refreshed and reviewed before use. The consumed activation claim cannot be reused. |
| B — runtime metadata verifier v3 and candidate11 executable evidence | Accept as the built-source and complete untimed-preflight prerequisite | None found. Timing still requires the owner's explicit, hash-bound one-attempt authorization and a coordinated quiet window. |

These dispositions confer no SDK qualification, resource-budget result, timing
result, global G5/G6 pass, retry, or automatic execution authority.

## A: repair and actual failure evidence

All 17 repair-freeze file hashes, the two live modified source hashes, and all
18 original campaign-evidence hashes matched. The worktree remains at
`de9c853cffdc7721be42e68a81415cfc1f63e6d4` with only `campaign.py` and
`test_harness.py` modified. Active `SOURCE-FREEZE.json` still binds the preserved
before-source. Eight relevant isolation/driver/worker/profile files still match
that active freeze.

Activation `38092184913` enrolled the created launchers as UID/GID 1001 with
`noNewPrivs=1` and zero effective/permitted/ambient capabilities. Exited-child
and WebKit controls passed; Chromium exited 1 with the preserved “Socket path
too long” failure. All three created groups and the parent were removed.
`ATTEMPT.json` is absent. The historical STOP incorrectly says `passed`; those
bytes and the consumed activation claim remain preserved.

For the actual root-supervisor/unprivileged-driver arrangement, the new short
scratch parent is supervisor-owned mode 0710 with the target primary GID; its
`w`, `h`, and `t` directories are driver-owned mode 0700. The driver's primary
GID provides traversal of the parent without write permission. Evidence and
configuration remain outside the disposable scratch directory. The reserved
socket spelling is checked against a 100-byte pathname budget before launch.
No browser sandbox, enrollment, cgroup accounting, schedule, timeout, or budget
change appears in the repair.

Cleanup requires the created driver to have stopped and the created group to
have been deleted. It checks the original parent/work device, inode, ownership
and modes, refuses unexpected entries/foreign devices or owners/special entries,
and uses descriptor-anchored symlink-resistant removal. It never follows links
to external directories. Failure retains scratch evidence and makes
`safeToContinue=false`. STOP now reports `failed`, the failure phase, preparation
outcome, and actual SDK-attempt-marker existence separately; parent cleanup
failure also produces failed STOP and exit 1.

One independent rerun of all 20 unchanged SDK-free controls passed, including
real local AF_UNIX binds, launch failure, cleanup refusal and external-link
preservation. Tests ran from copied source in this review directory. These local
controls do not establish root-to-UID Linux browser execution of the repair; that
is the obligation of the next separately reviewed activation. The original
sandbox-denied validation and later passing validation remain hashed inputs.
The unchanged 42-job schedule, 180-second trial limit, swap prohibition,
1,536 MiB absolute memory ceiling, 64/192/768 MiB ordinary incremental budgets
for small/catalog/large, and 8 MiB lifecycle drift policy retain their scope.

## B: exact semantics, executable chain, and attempt gate

All 69 v1 source/input files and 11 v2 source files match their original freezes.
The v3 diff adds fixture-derived `tags` and `deprecated` expectations for
candidate11 Node metadata and selects frozen, artifact-specific layouts. It
retains exact key-set, value-kind/content and wire size/hash checks. Original
Node and the explicit unchanged native metadata projection remain legacy; native
full inventories independently include the extended fields. Unknown, missing,
wrong, or incorrectly typed fields fail. The layout is not inferred from output.
Workers, clocks, retention/drop boundaries, loops, ledger and budgets are unchanged.

The first 14 untimed commands all exited 0. Offline v2 verification reproduces
the preserved metadata-layout failure. Applying the strict v3 oracle separately
to those preserved outputs passes all 77 cells per artifact without rewriting the
historical failed report. All 18 existing v3/prerequisite controls passed in this
review, including negative field, exact-number/token, output-coverage and clock
checks.

The second preflight contains two Node commands and twelve native commands over
the six registered profiles. All 14 command identities, launch/result records,
stdout/stderr hashes, executable identities and complete selected-job coverage
verified. Offline replay passes all 77 cells on each artifact. The 58-file spool
and preserved copy are byte-identical. Actual baseline and candidate edits on all
five edit-bearing profiles preserve untouched exact values and opaque token
bytes; their edited hashes match between artifacts. Preflight work/control each
uses one iteration, retains and consumes one output, and records null clock fields.

The review verified 2,478 frozen/nested file bindings, both package manifests,
all twelve archives per artifact against their extracted files (no extra files),
eleven crate VCS records per artifact, and matching npm build-info/artifact hashes.
Candidate identity is commit `5aee03054077ba7465acb024220027b5344faf29`; original
identity is `ba38c94c0da436f16cc9b4630d9d2ee2b896709b`. Cargo manifests point to
the reviewed v2 worker and corresponding packaged source paths. Pre-build and
post-build lockfiles match. Compiler/tool/version, release build command,
environment, build-output and copied-binary receipts agree; both native guard
receipts report two passed tests. This is a preserved hash/receipt provenance
chain, not an independent rebuild or compiler attestation.

`run-v3.py` rehashes frozen sources/artifacts/binaries, validates owner approval
and preflight/verifier/transcript hashes, and replays every actual semantic receipt
before creating the unchanged exclusive `ATTEMPT-CONSUMED.json` token. No token
exists. Native allocation admission remains before reservation with a 128 MiB
cap and truthful ZST accounting. Loop caps remain Node 10,000,000/native 1,000,000;
the preflight watchdog remains 120 seconds. Two warmups, all seven paired
observations, baseline-only count selection, no replacement samples and the
256 MiB prospective evidence cap remain unchanged.

Candidate first metadata produces more public output, so this acceptance does
not support a same-output engine-speedup claim. Cached getter work retains its
identity/cardinality workload with complete actual metadata checked. Component
boundaries and the original method-owner limitations remain controlling.

## Exact anchors and limits

| Input | SHA-256 |
| --- | --- |
| A `REPAIR-SOURCE-FREEZE.json` | `03f083eb899aaa9a9d1576aeef6c385fef754ebaaa52b2503fb7a3a71be9a744` |
| A `campaign.py` | `8fc7d832254e37dc97078e0f456f56e215d9d93f2691c2cc4a50a6c0807dca74` |
| A `test_harness.py` | `0dc8298309993fa2c783fe80d1c9cbc279ea3d5380f5c6637a2339431991db78` |
| B candidate11 manifest | `8985fac552ef7053ff0842d9a3c39356654ef4ecc6aed047bf22b191aab5ab47` |
| B `FROZEN-v3.json` | `721092fe054813a1b68c9f4a7b84e90eb0f6b168c0edb835130b24edb72c2989` |
| B `NATIVE-BINDINGS.json` | `c305ba6cf0ed3ed1c98738491ff259d30b565e34bf9e212b4b9416f7d9a4e142` |
| B semantic preflight | `30d53f265c15b65eef55b67e24b69c03c70055fafa8adee40be68a3e8623e414` |
| B verifier v3 | `d9b9edb6f90213a2f2fd16dfe3b710151dfc24ebeeacb7f8d5cd5647c033b889` |
| B driver v3 | `520de554920def94a069f39e3067e7331d5ed9efc74641aa248c5e47d8cdb14f` |
| B baseline binary | `a267a6f2ab2ce441020283c6dc31598f7d9892348889b5daa2a5ce9836475760` |
| B candidate binary | `d6fd49e67e9d5ebc240d170f61f0acede6ec61f169528772101b76969abd879e` |

`INPUTS.json` freezes the complete reviewed file identities; `REVIEW-FREEZE.json`
freezes this report and review outputs. No source repair, active seal refresh,
SDK/Cargo/browser/CI/cgroup execution, new corpus, benchmark or timing was
performed. No further review/execution/repair loop is requested by this report.
