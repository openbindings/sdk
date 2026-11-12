# Linux browser preparation proposal

This directory is a reviewable proposal, not an installed workflow or executed
preparation. Its only local execution is SDK-free admission/extraction/unit
validation. No SDK, Cargo, browser, cgroup control or measurement was executed.
The accepted harness in `../browser-kernel-prerequisites/` remains unchanged.
Candidate source and package bindings intentionally remain unfilled.

## Root-owned route

1. Review this source freeze. In a clean SDK worktree based on its declared
   integration branch, copy this directory to `.github/browser-memory/` and copy
   `browser-memory-pr.proposal.yml` to
   `.github/workflows/browser-memory-preparation.yml`. Preserve executable modes
   (in particular `seed/harness/enter.py`). This proposal has made neither copy.
2. Copy `REQUEST.template.json` to `REQUEST.json`. Fill the explicit root actor
   login, accepted candidate commit/tree/Rust-input SHA, and independently checked
   Node and Playwright download SHA256 values. The request fixes Node22.19.0,
   npm10.9.3, Playwright1.64.0, Rust1.99.0 and wasm-bindgen0.2.129. Each source
   checkout must match its full commit and tree. Expected new npm archive digests
   may remain null for preparation; both actual digests are mandatory for later
   activation. Never substitute an obsolete candidate because it is available.
3. Root pushes and opens/updates a same-repository PR using its normal authorized
   identity. `opened`, `synchronize` and `reopened` select preparation only. The
   new workflow remains branch-owned; `.github/workflows/ci.yml`, required checks,
   protection rules and permissions stay unchanged. No workflow was pushed here.
4. The Ubuntu24.04 x86_64 job validates inputs and read-only cgroup prerequisites,
   downloads pinned Node/Playwright, installs the selected browser files and OS
   dependencies, and builds two actual npm archives from exact clean checkouts.
   It uses `npm ci --ignore-scripts`, `build:wasm`, `build` and
   `npm pack --ignore-scripts`; it does not use the SDK qualification packager or
   execute SDK/browser jobs. Browser paths and versions come from installed
   Playwright metadata; WebKit's expected headless WPE ELF layout must exist.
5. Root downloads the successful artifact by exact run/attempt/artifact ID. Review
   `RESULT.json`, `PREPARATION.json`, `BINDINGS.proposed.json`, archive/source
   receipts, build-info, command logs and installed tree identities. Verify the
   downloaded `prepared-inputs.tar.gz` SHA256 and preserve it locally. The bundle
   restores only to `/opt/ob-memory-inputs`; binding paths are canonical there.
   Root must review the full outcome of every failed preparation as well.
6. Only after package/host review, create an independent `OWNER-REVIEW.json` using
   the accepted harness template. Copy `ACTIVATION.template.json` to `REQUEST.json`
   and fill every field, including both actual archives, exact downloaded bundle,
   binding, approval and preparer-freeze hashes, artifact metadata and a fresh
   32-hex activation ID. Push this change. A push in campaign mode does no work.
   Root applying `sdk-browser-memory-campaign-approved` is the separate final
   trigger. Do not apply it while a final candidate or owner review is pending.

The event gate requires the explicit actor, open same-repository PR, exact request
phase and run attempt 1. Campaign restore, artifact-claim upload and execution are
separate steps. The claim must be visible through the Actions API before the
unchanged harness runs. All jobs in this workflow share one concurrency group,
with no cancellation of a running job. An existing claim blocks another use of
that activation. Claim artifacts retain 90 days; root must keep a durable consumed-ID
ledger beyond retention and must never delete a claim to make a retry possible.
Platform cancellation or failed evidence upload is missing evidence, not success.

## What is frozen and what remains pending

`SEED-MANIFEST.json` lists only the exact accepted harness, its independent source
acceptance and SDK-free validation, and four registered synthetic fixture files.
Its harness freeze SHA256 is
`361ee880e6def05e826ecef1f800a4347b59de2e70ac954da7859cb0ff858825`.
No provider corpus, source checkout, Cargo cache or authentication environment is
included in the uploaded bundle. Explicit evidence globs preserve command logs
and all campaign/control outcomes; failed download metadata includes partial-byte
hashes when available. Partial downloads/build trees stay on the disposable runner
and are not represented as successful artifacts.

The original baseline archive remains
`80bceb3e1c71b5c6d6366c7aab5f8aae4481aed513df1cb1f4cdb3a496148994`,
original packaged commit `ba38c94c0da436f16cc9b4630d9d2ee2b896709b`.
The proposed reachable integration commit
`346da6523e2c87a7f2eb1b415ddeb77def7aab6c` has the same exact Git tree
`86ca4a150675ef4ea286c726409d98e6f18c1633`. Its expected Rust-input SHA is
`32a94299a2adfe9c14288a29cc8584777bba27002192d6ddcbb961f46ce712f6`.
A fresh Linux build is a **new archive from equivalent pinned source**, not the
original archive. Owner acceptance of this Linux baseline chain is still required;
the original package is never relabeled or replaced in historical evidence.

Preparation records Node/npm, Rust/compiler/bindgen and Git identities; exact
Playwright registry entries, browser file trees/launchers/actual executables;
Python, dpkg inventory and GitHub ImageVersion; package archive/extraction equality;
Wasm build-info; fixtures; proposal head commit and preparation source freeze.
Node and complete downloaded browser trees are hashed as well. Installed system
libraries are represented by the frozen dpkg inventory and runner image, not
bundled arbitrary system files.

The next runner must have the exact prepared ImageVersion. After installing the
same browser OS dependency set, the harness requires the same dpkg inventory and
Python identity. GitHub does not promise a particular future image for this runner
label. Image/apt drift, missing revisions, unavailable network dependencies,
insufficient runner disk or unsupported cgroup permissions are real operational
blockers: preserve the refusal and obtain a newly prepared/reviewed input set.
No binding is rewritten inside campaign activation. Fresh VM does not establish
quiet/exclusive hardware; this workflow makes no hosted timing claim.

Node and Playwright archive SHA256 inputs are deliberately unresolved here. Root
must independently obtain/check official bytes, record provenance and fill them;
no placeholder passes admission. Repository Actions policy may also require
approval or reject an action revision. Do not weaken policy to get a run.

## Bounds and phase separation

Preparation is limited to 90 minutes; the whole optional job to 165 minutes. Individual
commands have shorter deadlines. Timed-out commands target only their freshly
created process group; package-manager children use scoped sudo signaling. There
is no process-name cleanup, recursive cgroup cleanup, deletion of an existing
canonical tree, forced reclaim or GC. Archive validation precedes extraction into
a fresh unprivileged directory, rejecting traversal, duplicates, hardlinks,
devices, escaping links and oversized archives. No host/global cgroup setting is
changed. Ubuntu package installation belongs only to a future disposable job.

Later activation preserves the accepted harness's separate preparation,
CONTROL-ATTEMPT and SDK ATTEMPT records. Real exited-child 64 MiB and both actual
browser Worker hold/drop 64 MiB controls must pass before the 42 fixed SDK trials.
No real-control pass is claimed by local unit tests. Its 64/192/768 MiB incremental
budgets, 1.5 GiB ceiling, exact-owner checks and released 200-to-400 drift policy are
unchanged. Every adverse/inconclusive result remains visible; activation is not
permission to selectively retry. Kernel cgroup charges remain distinct from macOS
RSS, JS live heap and Wasm capacity.

## Official trigger references

GitHub documents PR activity types and checkout of `pull_request.head.sha`; it
also notes merge conflicts prevent PR workflows. This proposal uses that PR route
and does not depend on `workflow_dispatch` registration on the default branch.
See [events that trigger workflows](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).

The workflow requests only `contents: read` and `actions: read`. Root owns pushes
and labels; events produced with `GITHUB_TOKEN` have recursion/approval restrictions.
See [GITHUB_TOKEN authentication](https://docs.github.com/en/actions/tutorials/authenticate-with-github_token).
It never uses `pull_request_target`, repository write permission or a new secret.
Artifact upload uses [the pinned upload-artifact revision](https://github.com/actions/upload-artifact/tree/ea165f8d65b6e75b540449e92b4886f43607fa02).
Checkout is pinned to the SDK's existing revision
`11d5960a326750d5838078e36cf38b85af677262`; upload is pinned to
`ea165f8d65b6e75b540449e92b4886f43607fa02`.
These docs were inspected 2026-10-10; no remote workflow was installed or exercised.
