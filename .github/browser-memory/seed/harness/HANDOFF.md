# Linux browser group-memory prerequisites

Status: **source prepared; independent review, final bindings, Linux controls and
all SDK measurements pending**. No browser, SDK job, Cargo build, benchmark,
package installation, CI dispatch or global setting change was executed while
preparing this directory. Local SDK-free tests are not Linux-control passes.

Authority: [prospective profile decision](../resources/BROWSER-KERNEL-PROFILE-DECISION.md)
and [GATES.md](../../../../GATES.md). This directory is an additional Linux profile;
it does not revise the preserved macOS result or required component CI.

## Reviewable source

- `campaign.py`: admission, three controls, one SDK ATTEMPT, fixed 42-job schedule,
  per-job deadlines, raw evidence, and cleanup restricted to created resources.
- `kernel.py`: cgroup counter parsing, process evidence, numerical decisions.
- `identity.py`, `inspect-inputs.py`: package/archive/source/runtime admission and
  read-only identity preparation. No implicit installation or source builds.
- `enter.py`: small unprivileged browser wrapper; `driver.mjs`: external
  Playwright driver and HTTP server; `worker.mjs`: actual Worker public jobs.
- `touch.py`: SDK-free 64 MiB exited-child control.
- `PROFILES.json`: unchanged registered small/catalog/large hashes and facts;
  `replacement-fixture.json`: unchanged candidate04 replacement fixture.
- `test_harness.py`: synthetic instrumentation/admission tests and a real local
  SDK-free payload handshake. Counter fixtures are fabricated **test inputs** and
  cannot be used as campaign receipts.

`SOURCE-FREEZE.json` binds final source, tests, templates, protocol, and the final
local validation receipt. Earlier validation snapshots remain additive evidence.
Changing source invalidates owner review and requires a new source freeze.

## Isolation and measurement

Ubuntu 24.04 x86_64 is the only admitted host. The root supervisor first confirms
the existing unified cgroup-v2 mount and that its root already enables memory.
It refuses unsupported hosts; it never mounts a controller, changes root
`subtree_control`, runs `swapoff`, changes sysctls, or clears caches.

The supervisor creates one uniquely named root-owned campaign group, enables
memory only there, and creates a fresh root-owned leaf for every control/trial.
Both parent and leaves have `memory.max=1610612736`, `memory.swap.max=0`; leaves
have `memory.oom.group=1`. The parent bounds lingering group charges across the
campaign. Parent limit/swap events also block qualification. No driver or HTTP
server process enters those groups.

The driver runs as the supplied nonroot uid/gid with supplementary groups removed.
Playwright executes `enter.py` as its browser executable. The wrapper sets Linux
`PR_SET_NO_NEW_PRIVS`, then contacts a root-owned socket. The supervisor checks
SO_PEERCRED, a one-use random token, uid/gid, ancestry under the created driver,
zero effective capabilities and no-new-privileges. It moves only that waiting
wrapper, verifies the unchanged PID start identity and group membership, then
acknowledges. Only then does the wrapper exec the pinned browser launcher.
Chromium's debugging pipe descriptors remain inherited; no sudo occurs in that
launch path. WebKit's pinned launcher can spawn its pinned actual executable.
Actual group membership and executable identity are checked at Worker phases.
Children inherit membership and cannot gain sudo/setuid privileges or write the
root-owned cgroup migration files. Observed escapes fail the witness.

The small Python wrapper's initial interpreter allocations precede enrollment;
browser executable loading, startup and descendants follow it. This is explicitly
launch-before-**browser**-allocation, not a claim that the wrapper interpreter's
earlier allocations moved. The group records kernel charges, including anonymous,
file and kernel categories. Browser files read during identity hashing and other
already resident shared pages can be charged elsewhere. These measurements cannot
be described as summed RSS, all physical browser memory, live JS heap, or the
macOS lifetime RSS profile. The VM is fresh, not exclusive physical hardware.

Each snapshot preserves the raw `memory.current`, `memory.peak`, `memory.stat`,
`memory.events`, `memory.swap.current`, `memory.swap.peak`, `memory.swap.events`,
limits, `cgroup.events` and `cgroup.procs`. Partial/malformed snapshots are retained
and fail admission/judgment. Process PID, parent, start identity, executable,
membership and privilege state accompany observations. Samples occur at explicit
Worker handshakes and once per second; the kernel lifetime peak covers intervening
allocations and exited descendants. Peak is never reset. Terminal counters are
saved after processes exit and before group deletion.

Warm idle is the current charge after complete public paths have warmed and all
SDK owners have been released. Incremental peak is
`max(0, fresh-group lifetime memory.peak - warmed-idle memory.current)`.
Cold startup or warmup can dominate this conservative value; it is never
subtracted away. Absolute lifetime peak is always retained. Any observed swapping,
swap-limit event, `max`, `oom`, `oom_kill`, exceeded ceiling, malformed evidence,
timeout or kill prevents a pass. Swap is disabled only for created groups; host
swap configuration and activity files are recorded.

## Controls and jobs

All three prerequisite controls run once and preserve their outcomes, unless an
unsafe cleanup or interruption prevents further execution. A fresh SDK-free child
touches 64 MiB, waits while held, drops it and exits. The parent must observe the
charge growth and see its lifetime peak after exit in an empty group. Each actual
browser then runs a fresh SDK-free Worker that fills a 64 MiB ArrayBuffer, checks
its checksum, holds it through the counter acknowledgment, drops both references,
and yields twice. The preregistered held-current growth admission range is
48–160 MiB for both controls: 16 MiB tolerance below the allocation and bounded
runtime overhead above it. A held-current signal does not prove physical
collection after references are dropped. Owner review must accept this numerical
control criterion prospectively; it is not derived from observed Linux results.

The fixed SDK schedule is 36 ordinary jobs: baseline and final candidate ×
small/catalog/large × Chromium/WebKit × three fresh-process trials, with artifact
order alternating by trial. It adds six candidate lifecycle jobs, three per
browser. Ordinary jobs use the frozen registered document bytes and public
parse-document → assess → context/alias contract preparation → exact valid/invalid
value admission → verdict/diagnostic serialization → explicit release path.
Operations, conformance and verdicts are checked; no required valid job may refuse.
Two full warmups precede one complete recorded job. The incremental budgets remain
64/192/768 MiB respectively; the absolute ceiling remains 1.5 GiB.

Lifecycle uses 20 complete warmups, one fixed owner baseline, 1,000 retained calls,
and exactly 400 alternating same-URI replacements. It uses the shipped default
cache with six extra preparations, checks retained-old/new isolation, exercises
pre-cancelled prepare/validate followed by healthy reuse, and checks exact retained
and released owner counts. A temporary exact-Wasm-hash instantiate hook records
one attributable memory's capacity; the original hook is restored immediately.
Package bytes remain unchanged. No capacity observation substitutes for a kernel
measurement.

**Review the release-checkpoint choice:** candidate04 retained one original
contract at its 200/400 snapshots. This harness retains an original const0
contract for replacements 1–200, releases it at 200, observes the exact warmed
owner baseline, then prepares an equivalent const0 contract for 201–400 and
releases it at 400. Thus both kernel-current checkpoints are fully released while
old-contract independence is exercised in both equal epochs. This is a disclosed
instrumentation change, not retroactive relabeling of candidate04. Any >8 MiB
released-current 200→400 rise marks that trial investigation-required and blocks
automatic qualification. Review all three fresh trials for reproducibility and
attribute growth; do not retry selected trials or force GC/reclaim.

## Preparation, invocation and remaining prerequisites

`BINDINGS.template.json` deliberately contains null baseline/candidate/browser/host
identities. `OWNER-REVIEW.template.json` is pending and deliberately cannot admit a
campaign. The accepted final candidate is not stable yet. Do not bind candidate04
or another intermediate package merely to make this harness executable.

Prepare these in a separate Linux preparation activity, preserving its receipts:

1. Resolve the original pre-change baseline from the retained
   `rounds/01/qualification/inputs/runtime-v1-IDENTITY.json` and its package/source
   chain. That record points to original npm archive SHA256
   `80bceb3e1c71b5c6d6366c7aab5f8aae4481aed513df1cb1f4cdb3a496148994`.
   Bind the accepted final candidate only after integration/package stabilization.
2. Supply immutable npm archives, exact extractions, full source commit/tree and
   reviewed source-package receipts. Each artifact object has `sourceCommit`,
   `sourceTree`, `sourceReceipt:{path,sha256}`, `archive:{path,sha256}`,
   `packageRoot`, `treeSha256`, and `wasmSha256`. A normalized source receipt must
   contain the same `sourceCommit`, `sourceTree` and `npmArchiveSha256`, cite the
   original build provenance, and be checked by the independent owner. Merely
   writing matching values is not proof that source produced an archive.
3. Prepare the exact Linux Node executable, system `/usr/bin/python3` target,
   Playwright tree, both browser trees/launchers/actual binaries, and their Linux
   dependencies. Archive package files are compared exactly to extracted bytes;
   complete browser/Playwright trees and executable hashes are checked. Symlinks
   escaping identity roots and nonregular npm archive entries are refused.
   `inspect-inputs.py PATH...` computes the canonical tree/file records without
   running browsers. Browser revision/version must match Playwright's registry.
4. Record the Ubuntu runner `ImageVersion` and the exact `dpkg-query -W` listing
   hash after installing prerequisites. Bind all paths at their canonical Linux
   destinations; the proposed bundle uses `/opt/ob-memory-inputs`. A later fresh
   runner image or changed dependency set fails preflight rather than rebinding
   itself. Node version, Python version, OS/kernel/CPU/memory/swap are retained.
5. Copy the unchanged three fixture files from `rounds/01/registration/fixtures/`
   and the replacement fixture from this directory. Preserve the registered
   expected facts and their original oracle status; reconcile semantic authority
   during review, without inventing independent-oracle credit.
6. Obtain independent owner review of source, tests, identities, numerical controls,
   launch isolation, cleanup, scope and checkpoint design. The approval must bind
   both `SOURCE-FREEZE.json` and the exact completed `BINDINGS.json` hashes. No
   approval is currently present. Review templates are never treated as receipts.

On the prepared Ubuntu VM, a read-only host/package admission is:

```sh
sudo -n --preserve-env=ImageOS,ImageVersion,RUNNER_OS,RUNNER_ARCH,GITHUB_RUN_ID,GITHUB_RUN_ATTEMPT \
  /usr/bin/python3 /opt/ob-memory-inputs/harness/campaign.py \
  --bindings /opt/ob-memory-inputs/BINDINGS.json \
  --review /opt/ob-memory-inputs/OWNER-REVIEW.json \
  --uid "$(id -u)" --gid "$(id -g)" \
  --output "$RUNNER_TEMP/browser-memory-preflight" --preflight-only
```

After review and host preparation, remove `--preflight-only` and use a **new**
output path for the single whole campaign. Preparation must pass before a
`CONTROL-ATTEMPT.json` is written. All three real controls must pass and all frozen
inputs must revalidate before `ATTEMPT.json` is consumed for the SDK campaign.
The output directory is created exclusively; no resume/overwrite/selected-job
option exists. A failed SDK trial does not omit later jobs. Unsafe cleanup or an
interruption records remaining jobs as unexecuted. Preserve the whole output.

The standalone `ubuntu-workflow.proposal.yml` is a dispatch-only GitHub-hosted
route, not an installed workflow. It downloads a specific reviewed preparation
artifact by run ID and SHA256, restores it at the frozen path, installs the pinned
Playwright release's OS prerequisites, then insists that actual image/dependency
identities match the previously reviewed binding. Preparation defects require a
new additive preparation/binding review; do not keep changing versions in a
measurement attempt. The proposed upload action revision must also be checked by
the installing owner. Existing required CI checks remain unchanged.

Each browser job has a 150-second Worker watchdog and a 180-second external
supervisor deadline; the SDK-free child has 30 seconds. Cleanup has a bounded
10-second wait for empty groups and uses `cgroup.kill` only after checking the
created group's name, inode/device, ownership, and absence of unknown subgroups.
Only the created driver Popen process is killed directly. No PID guessing,
recursive cgroup deletion, unrelated process kill, automatic retry, global GC,
reclaim, or cache dropping is used. Raw stdout/stderr have a 16 MiB per-job cap;
exceeding it preserves the observed prefix and fails the witness. All completed
and adverse outcomes, preparation errors and final cleanup receipts are retained.

## Validation and claim limits

Read `SOURCE-FREEZE.json` for the final validation receipt. Local checks cover
Python/JavaScript syntax, 42-job schedule, malformed/missing counters, OOM/swap
rejection, cold-peak accounting, exact owners, >8 MiB drift, incomplete/GC evidence,
control charge criteria, ancestry cycles, unsafe cleanup rejection, archive/tree
identity, source tampering and SDK-free payload acknowledgment.

The Linux cgroup APIs, authenticated enrollment, actual Chromium/WebKit launch,
no-new-privileges compatibility, control charge ranges, and all SDK results remain
**unexecuted**. Those are precisely what the real pre-SDK controls and independent
review must establish. No G6, macOS RSS, physical collection, universal heap bound,
leak absence outside this experiment, mobile or deployed Cloudflare claim follows.

Method sources read during preparation:
- https://www.kernel.org/doc/html/v6.6/admin-guide/cgroup-v2.html
- https://docs.github.com/en/actions/reference/runners/github-hosted-runners

Public-job source references (read only):
- `openbindings/sdk/tools/runtime-qualification/workloads.mjs`
- `openbindings/sdk/tools/runtime-qualification/browser.mjs`
- `rounds/01/continuation/browser-resource-candidate04/worker-v1.mjs`
- `rounds/01/registration/generated-profiles.json`
