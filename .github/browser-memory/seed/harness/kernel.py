"""Linux-only cgroup instrumentation. Unit fixtures never constitute host evidence."""
import hashlib
import json
import os
from pathlib import Path
import re
import time
import uuid

MIB = 1024 * 1024
CEILING = 1536 * MIB
COUNTERS = ("memory.current", "memory.peak", "memory.stat", "memory.events",
            "memory.swap.current", "memory.swap.peak", "memory.swap.events",
            "memory.max", "memory.swap.max", "cgroup.events", "cgroup.procs")


def require(value, message):
    if not value:
        raise ValueError(message)


def uint(raw):
    require(isinstance(raw, str) and re.fullmatch(r"[0-9]+\n?", raw), "malformed unsigned counter")
    return int(raw)


def pairs(raw, required):
    out = {}
    for line in raw.splitlines():
        fields = line.split()
        require(len(fields) == 2 and fields[0] not in out, "malformed/duplicate counter key")
        out[fields[0]] = uint(fields[1])
    require(set(required) <= out.keys(), "missing counter fields: " + str(set(required) - out.keys()))
    return out


def parse_snapshot(raw):
    require(set(raw) == set(COUNTERS), "missing/extra snapshot files")
    out = {key: uint(raw[key]) for key in (
        "memory.current", "memory.peak", "memory.swap.current", "memory.swap.peak",
        "memory.max", "memory.swap.max")}
    out["memory.stat"] = pairs(raw["memory.stat"], ("anon", "file", "kernel", "pgfault"))
    out["memory.events"] = pairs(raw["memory.events"], ("low", "high", "max", "oom", "oom_kill"))
    out["memory.swap.events"] = pairs(raw["memory.swap.events"], ("high", "max", "fail"))
    out["cgroup.events"] = pairs(raw["cgroup.events"], ("populated", "frozen"))
    out["cgroup.procs"] = sorted(set(uint(x) for x in raw["cgroup.procs"].splitlines()))
    require(out["cgroup.events"]["populated"] in (0, 1), "invalid populated state")
    require(out["cgroup.events"]["frozen"] == 0, "unexpected frozen group")
    require(out["memory.max"] == CEILING and out["memory.swap.max"] == 0, "safety limits changed")
    # Sequential reads can race with allocation; peak is read AFTER current by snapshot().
    require(out["memory.peak"] >= out["memory.current"], "peak below current")
    return out


def healthy(rows):
    for row in rows:
        p = row["parsed"]
        require(p["memory.peak"] <= CEILING, "absolute ceiling exceeded")
        require(all(p["memory.events"][k] == 0 for k in ("max", "oom", "oom_kill")), "limit/OOM event")
        require(p["memory.swap.current"] == p["memory.swap.peak"] == 0, "swap activity")
        require(not any(p["memory.swap.events"].values()), "swap limit/activity event")


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def proc(pid):
    root = Path("/proc") / str(pid)
    stat = (root / "stat").read_text()
    # The command field may contain spaces/parentheses.
    parts = stat[stat.rfind(")") + 2:].split()
    status = dict(line.split(":", 1) for line in (root / "status").read_text().splitlines())
    return {"pid": pid, "ppid": int(parts[1]), "startTicks": int(parts[19]),
            "membership": (root / "cgroup").read_text(),
            "uids": [int(x) for x in status["Uid"].split()],
            "noNewPrivs": int(status["NoNewPrivs"]),
            "capEff": status["CapEff"].strip(),
            "capPrm": status["CapPrm"].strip(), "capAmb": status["CapAmb"].strip(),
            "executable": os.readlink(root / "exe")}


def descendant(pid, ancestor):
    visited = set()
    while pid > 1 and pid not in visited:
        if pid == ancestor:
            return True
        visited.add(pid)
        pid = proc(pid)["ppid"]
    return False


class Group:
    """Only a newly created, root-owned leaf can be killed or removed."""
    def __init__(self, parent):
        self.path = Path(parent) / ("ob-memory-" + uuid.uuid4().hex)
        self.path.mkdir(mode=0o755)  # never exist_ok
        self.identity = (self.path.stat().st_dev, self.path.stat().st_ino)
        self.rows = []
        self.known = {}
        try:
            (self.path / "memory.max").write_text(str(CEILING))
            (self.path / "memory.swap.max").write_text("0")
            (self.path / "memory.oom.group").write_text("1")
            self.membership = f"0::/{self.path.relative_to('/sys/fs/cgroup')}\n"
            require((self.path / 'cgroup.kill').is_file(), 'cgroup.kill unavailable')
            for name in ('cgroup.procs', 'cgroup.kill'):
                st = (self.path / name).stat()
                require(st.st_uid == 0 and st.st_mode & 0o022 == 0, 'cgroup migration/cleanup file is writable by nonroot')
        except Exception:
            self.guard()
            self.path.rmdir()  # no process has been authorized to enter yet
            raise

    def guard(self):
        require(not self.path.is_symlink(), "unsafe cleanup: symlink group")
        st = self.path.stat()
        require((st.st_dev, st.st_ino) == self.identity and st.st_uid == 0,
                "unsafe cleanup: group identity/owner changed")
        require(re.fullmatch(r"ob-memory-[0-9a-f]{32}", self.path.name), "unsafe group name")
        require(not any(x.is_dir() for x in self.path.iterdir()), "unexpected nested groups")

    def snapshot(self, phase, worker, driver_pid=None):
        self.guard()
        row = {"phase": phase, "monotonicNs": time.monotonic_ns(), "raw": {},
               "parsed": None, "processes": [], "worker": worker}
        self.rows.append(row)  # retain partial/malformed evidence before rejecting it
        for name in COUNTERS:
            row['raw'][name] = (self.path / name).read_text()
        parsed = parse_snapshot(row['raw'])
        row['parsed'] = parsed
        members = []
        for pid in parsed["cgroup.procs"]:
            try:
                p = proc(pid)
            except FileNotFoundError:
                members.append({"pid": pid, "exitedDuringSnapshot": True})
                continue
            require(p["membership"] == self.membership, "process migrated/foreign subgroup")
            self.known[(pid, p["startTicks"])] = p
            members.append(p)
        for (pid, start), old in list(self.known.items()):
            try:
                current = proc(pid)
            except FileNotFoundError:
                continue
            if current["startTicks"] == start:
                require(current["membership"] == self.membership, "observed process escaped group")
        if driver_pid:
            require(proc(driver_pid)["membership"] != self.membership, "driver/server charged to browser group")
        row['processes'] = members
        return row

    def cleanup(self):
        self.guard()
        # cgroup.kill acts on this recorded leaf atomically; never kill by a stale PID.
        (self.path / "cgroup.kill").write_text("1")
        deadline = time.monotonic() + 10
        while pairs((self.path / "cgroup.events").read_text(), ("populated",))["populated"]:
            require(time.monotonic() < deadline, "created group failed to empty; left for review")
            time.sleep(.05)
        terminal_error = None
        try:
            self.snapshot("terminal-before-delete", None)
        except Exception as error:
            terminal_error = error
        self.path.rmdir()
        self.deleted = True
        if terminal_error is not None:
            raise terminal_error


def schedule():
    jobs = []
    for trial in (1, 2, 3):
        for browser in ("chromium", "webkit"):
            for profile in ("small", "catalog", "large"):
                for artifact in (("baseline", "candidate") if trial % 2 else ("candidate", "baseline")):
                    jobs.append(dict(browser=browser, mode="ordinary", profile=profile, artifact=artifact, trial=trial))
            jobs.append(dict(browser=browser, mode="lifecycle", artifact="candidate", trial=trial))
    return [dict(j, id="-".join(str(j.get(k, "")) for k in ("browser", "artifact", "mode", "profile", "trial"))) for j in jobs]


def judge(job, rows, result):
    require(result["status"] == "passed", "Worker/driver failure")
    healthy(rows)
    by_phase = {r["phase"]: r for r in rows}
    observed = [r["phase"] for r in rows if r["worker"] is not None]
    expected = (["idle", "held-64MiB", "dropped-references"] if job["mode"] == "control" else
                ["warm-released", "all-released"] if job["mode"] == "ordinary" else
                ["warm-released", "1000-retained-calls", "released-200", "released-400", "all-released"])
    require(observed == expected, "missing/out-of-order Worker phases")
    worker = result["worker"]
    require(worker["status"] == "passed" and worker["forcedGc"] is False and worker["timed"] is False,
            "invalid Worker completion evidence")
    if job["mode"] == "control":
        require(worker["sdkLoaded"] is False, "SDK-free control loaded SDK")
        a, b = (by_phase[p] for p in ("idle", "held-64MiB"))
        require(b["worker"]["payloadBytes"] == 64 * MIB and b["worker"]["checksum"] == 278528, "bad control allocation/checksum")
        delta = b["parsed"]["memory.current"] - a["parsed"]["memory.current"]
        require(48 * MIB <= delta <= 160 * MIB, "64MiB current-charge control outside preregistered range")
        require(by_phase["dropped-references"]["worker"]["payloadBytes"] == 0, "control references retained")
        return {"status": "passed", "heldGrowthBytes": delta, "physicalCollectionClaim": False}
    baseline = by_phase["warm-released"]["worker"]["storageOwners"]
    require(type(baseline) is int and baseline >= 0, "missing exact warmed owners")
    require(worker["sdkLoaded"] is True and worker["fixedWarmedOwners"] == baseline, "missing SDK baseline")
    for r in rows:
        if r["worker"] is not None:
            require(type(r["worker"]["storageOwners"]) is int and r["worker"]["storageOwners"] >= 0,
                    "invalid exact owner evidence")
            require(type(r["worker"]["wasmCapacityBytes"]) is int and r["worker"]["wasmCapacityBytes"] > 0,
                    "missing attributed Wasm capacity")
    for r in rows:
        if r["phase"] in ("all-released", "released-200", "released-400"):
            require(r["worker"]["storageOwners"] == baseline, "exact owner release mismatch")
    peak = max(r["parsed"]["memory.peak"] for r in rows)
    idle = by_phase["warm-released"]["parsed"]["memory.current"]
    incremental = max(0, peak - idle)
    out = {"absoluteLifetimePeakBytes": peak, "warmedIdleBytes": idle,
           "conservativeIncrementalPeakBytes": incremental, "status": "passed"}
    if job["mode"] == "ordinary":
        require(incremental <= dict(small=64, catalog=192, large=768)[job["profile"]] * MIB,
                "ordinary incremental peak budget exceeded")
        require("all-released" in by_phase, "missing final release")
    else:
        require(worker["retainedCalls"] == 1000 and worker["replacements"] == 400, "lifecycle job incomplete")
        a, b = (by_phase[p] for p in ("released-200", "released-400"))
        out["releasedCurrentDelta200To400Bytes"] = b["parsed"]["memory.current"] - a["parsed"]["memory.current"]
        out["wasmCapacityDelta200To400Bytes"] = b["worker"]["wasmCapacityBytes"] - a["worker"]["wasmCapacityBytes"]
        if out["releasedCurrentDelta200To400Bytes"] > 8 * MIB:
            out["status"] = "investigation-required"
    return out
