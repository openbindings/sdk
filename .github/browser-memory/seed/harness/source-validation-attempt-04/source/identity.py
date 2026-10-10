"""Read-only admission. No downloads, package extraction, build or browser launch."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import sys
import tarfile
from kernel import require, sha

HERE = Path(__file__).resolve().parent


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def tree(root):
    root = Path(root).resolve(strict=True)
    rows = []
    for p in sorted(root.rglob("*")):
        rel = p.relative_to(root).as_posix()
        if p.is_symlink():
            target = p.resolve(strict=True)
            require(target.is_relative_to(root), "tree symlink escapes root: " + rel)
            rows.append({"path": rel, "link": os.readlink(p)})
        elif p.is_file():
            rows.append({"path": rel, "bytes": p.stat().st_size, "sha256": sha(p),
                         "executable": bool(p.stat().st_mode & 0o111)})
        else:
            require(p.is_dir(), "special file in identity tree")
    require(rows, "empty identity tree")
    return hashlib.sha256(canonical(rows)).hexdigest(), rows


def file_binding(row):
    require(isinstance(row, dict) and isinstance(row.get("path"), str), "missing file binding")
    require(re.fullmatch(r"[0-9a-f]{64}", row.get("sha256", "")), "invalid file digest")
    p = Path(row["path"])
    require(p.is_absolute() and p.resolve(strict=True) == p, "file path must be canonical absolute")
    require(sha(p) == row["sha256"], "file identity mismatch: " + str(p))
    return p


def package(row):
    require(isinstance(row, dict), "baseline/final candidate is not bound")
    for k in ("sourceCommit", "sourceTree"):
        require(re.fullmatch(r"[0-9a-f]{40}", row.get(k, "")), "missing full source identity")
    receipt = json.loads(file_binding(row["sourceReceipt"]).read_text())
    require(receipt["sourceCommit"] == row["sourceCommit"] and receipt["sourceTree"] == row["sourceTree"],
            "source receipt disagrees")
    require(receipt["npmArchiveSha256"] == row["archive"]["sha256"], "source/archive chain missing")
    archive = file_binding(row["archive"])
    root = Path(row["packageRoot"])
    require(root.is_absolute() and root.resolve(strict=True) == root, "unsafe package root")
    digest, files = tree(root)
    require(digest == row["treeSha256"] and all("link" not in f for f in files), "package tree changed/linked")
    actual = {x["path"]: x["sha256"] for x in files}
    packed = {}
    with tarfile.open(archive, "r:gz") as t:
        for member in t:
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and ".." not in path.parts and path.parts[0] == "package", "unsafe archive path")
            if member.isdir():
                continue
            require(member.isfile(), "nonregular npm archive entry")
            rel = str(PurePosixPath(*path.parts[1:]))
            require(rel not in packed and member.size <= 128 * 1024 * 1024, "duplicate/oversized archive entry")
            packed[rel] = hashlib.sha256(t.extractfile(member).read()).hexdigest()
    require(actual == packed, "extracted npm files differ from archive")
    require(actual["dist/wasm/openbindings_wasm_bg.wasm"] == row["wasmSha256"], "Wasm binding mismatch")
    return files


def source_freeze():
    freeze = json.loads((HERE / "SOURCE-FREEZE.json").read_text())
    require(freeze["schemaVersion"] == 1 and freeze["files"], "missing source freeze")
    required = {'campaign.py', 'kernel.py', 'identity.py', 'enter.py', 'driver.mjs', 'worker.mjs',
                'touch.py', 'inspect-inputs.py', 'test_harness.py', 'PROFILES.json', 'replacement-fixture.json',
                'BINDINGS.template.json', 'OWNER-REVIEW.template.json', 'HANDOFF.md', 'ubuntu-workflow.proposal.yml'}
    require(required <= freeze['files'].keys(), 'source freeze omits required harness files')
    for name, digest in freeze["files"].items():
        p = HERE / name
        require(p.resolve().is_relative_to(HERE) and sha(p) == digest, "harness source changed: " + name)
    return sha(HERE / "SOURCE-FREEZE.json")


def bindings(path, review_path):
    freeze_hash = source_freeze()
    b = json.loads(Path(path).read_text())
    require(b["schemaVersion"] == 1, "unsupported bindings schema")
    # Approval is checked before executing even the pinned --version programs.
    review = json.loads(Path(review_path).read_text())
    require(review["status"] == "approved" and review["independentOfHarnessImplementation"] is True,
            "independent owner review required")
    require(isinstance(review["reviewer"], str) and review["reviewer"].strip(), "reviewer missing")
    require(review["sourceFreezeSha256"] == freeze_hash and review["bindingsSha256"] == sha(path), "review covers different frozen inputs")
    require(review["acceptsKernelChargeScope"] is True and review["acceptsAdmissionAndCleanup"] is True,
            "scope/admission/cleanup review missing")
    require(set(b["artifacts"]) == {"baseline", "candidate"}, "both artifacts required")
    files = {name: package(row) for name, row in b["artifacts"].items()}
    for name in ("node", "python"):
        executable = file_binding(b[name])
        result = subprocess.run([str(executable), "--version"], capture_output=True, text=True, timeout=10, check=True)
        require(result.stdout.strip() == b[name]["version"], "runtime version changed")
    require(Path(sys.executable).resolve() == Path(b["python"]["path"]), "supervisor Python identity mismatch")
    require(Path('/usr/bin/python3').resolve() == Path(b['python']['path']), "wrapper shebang must resolve to pinned Python")
    require(os.access(HERE / 'enter.py', os.X_OK), "browser wrapper is not executable")
    pw = b["playwright"]
    require(tree(pw["root"])[0] == pw["treeSha256"], "Playwright tree changed")
    require(json.loads((Path(pw["root"]) / "package.json").read_text())["version"] == pw["version"], "Playwright version mismatch")
    require(set(b["browsers"]) == {"chromium", "webkit"}, "both browsers required")
    browser_registry = json.loads((Path(pw["root"]) / "browsers.json").read_text())["browsers"]
    for name, browser in b["browsers"].items():
        require(tree(browser["root"])[0] == browser["treeSha256"], "browser tree changed: " + name)
        require(browser["revision"] and browser["version"], "browser revision/version missing")
        registry = next(r for r in browser_registry if r['name'] == browser['registryName'])
        require(browser['registryName'] in (('chromium', 'chromium-headless-shell') if name == 'chromium' else ('webkit',)), "wrong browser registry entry")
        require(registry.get('revisionOverrides', {}).get('ubuntu24.04-x64', registry['revision']) == browser['revision'], "browser revision disagrees with pinned Playwright")
        require(registry['browserVersion'] == browser['version'], "browser version disagrees with pinned Playwright")
        for key in ("launcher", "actualExecutable"):
            p = file_binding(browser[key])
            require(p.is_relative_to(Path(browser["root"]).resolve()), "browser executable outside pinned tree")
    profiles = json.loads((HERE / "PROFILES.json").read_text())
    require(set(b["fixtures"]) == set(profiles), "fixture set differs")
    for name, fixture in b["fixtures"].items():
        require(fixture["sha256"] == profiles[name]["sha256"], "registered fixture hash changed")
        file_binding(fixture)
    return b, files, {"sourceFreezeSha256": freeze_hash, "bindingsSha256": sha(path), "reviewSha256": sha(review_path)}


def host(uid, gid):
    require(sys.platform == "linux" and os.geteuid() == 0, "Linux root supervisor required; no fallback")
    require(platform.machine() == 'x86_64', 'profile requires Ubuntu24.04 x86_64')
    release = Path('/etc/os-release').read_text()
    require('ID=ubuntu\n' in release and 'VERSION_ID="24.04"' in release, 'profile requires Ubuntu24.04')
    require(type(uid) is int and uid > 0 and type(gid) is int and gid > 0, "nonroot browser/driver uid and gid required")
    root = Path("/sys/fs/cgroup")
    require(root.resolve() == root, "unexpected cgroup mount path")
    migration = (root / 'cgroup.procs').stat()
    require(migration.st_uid == 0 and migration.st_mode & 0o022 == 0, 'common ancestor migration permission is unsafe')
    mountinfo = Path("/proc/self/mountinfo").read_text()
    matches = [line for line in mountinfo.splitlines() if line.split()[4] == str(root)]
    require(len(matches) == 1 and " - cgroup2 " in matches[0], "unified cgroup2 mount required")
    require("memory" in (root / "cgroup.controllers").read_text().split(), "memory controller unavailable")
    require("memory" in (root / "cgroup.subtree_control").read_text().split(), "root memory controller is not enabled; refusing global change")
    dpkg = subprocess.run(['/usr/bin/dpkg-query', '-W', '-f=${binary:Package}\t${Version}\n'],
                          capture_output=True, text=True, check=True, timeout=15).stdout
    return {"platform": platform.platform(), "uname": list(platform.uname()), "mountinfo": mountinfo,
            "osRelease": Path("/etc/os-release").read_text(), "procSwaps": Path("/proc/swaps").read_text(),
            "meminfo": Path("/proc/meminfo").read_text(), "cpuinfo": Path("/proc/cpuinfo").read_text(),
            "supervisorCgroup": Path("/proc/self/cgroup").read_text(), "uid": uid, "gid": gid,
            "dpkg": dpkg, "dpkgSha256": hashlib.sha256(dpkg.encode()).hexdigest(),
            "runner": {k: os.environ.get(k) for k in ("ImageOS", "ImageVersion", "RUNNER_OS", "RUNNER_ARCH", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT")}}
