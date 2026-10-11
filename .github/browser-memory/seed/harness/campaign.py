#!/usr/bin/env python3
"""One prospective campaign: admission -> controls -> ATTEMPT -> all 42 jobs.

Run only after independent owner approval of the frozen bindings. No retries.
Root supervises; Node/HTTP/browser run as the supplied nonroot account.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import selectors
import shutil
import signal
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from identity import HERE, bindings, host, sha
from kernel import CEILING, COUNTERS, MIB, Group, descendant, healthy, judge, parse_snapshot, proc, require, schedule

STOP_REQUESTED = False


def stop_requested(signum, frame):
    global STOP_REQUESTED
    STOP_REQUESTED = True
    raise InterruptedError('campaign interrupted by signal ' + str(signum))


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")
    Path(path).chmod(0o644)


def stop_receipt(preparation, output, error, phase):
    return {**preparation, "status": "failed", "preparationPassed": preparation["status"] == 'passed',
            "failurePhase": phase, "error": repr(error),
            "sdkAttemptConsumed": (output / "ATTEMPT.json").exists()}


class Scratch:
    """Short private scratch; receipts stay outside this disposable directory.

    The supervisor owns the non-writable parent. The driver owns only w/ and
    cannot replace that entry. Cleanup requires stopped processes and descriptor-
    anchored, symlink-resistant rmtree; no fallback to path-based recursion.
    """
    # The observed pinned Chromium suffix uses six random ASCII characters.
    # Also reserve the longer Playwright profile spelling for socket preflight.
    SOCKET_SUFFIX = '/playwright_chromiumdev_profile-XXXXXX/SingletonSocket'

    def __init__(self, uid, gid):
        self.uid, self.gid = uid, gid
        self.root = None
        self.fd = None
        self.info = {"created": False, "removed": False, "ready": False}

    def create(self):
        base = Path('/tmp').resolve(strict=True)
        st = base.stat()
        require(st.st_uid == 0 and stat.S_ISDIR(st.st_mode) and st.st_mode & stat.S_ISVTX,
                'unsafe scratch parent; root-owned sticky temporary directory required')
        require(shutil.rmtree.avoids_symlink_attacks, 'descriptor-safe scratch cleanup unavailable')
        self.root = Path(tempfile.mkdtemp(prefix='obm-', dir=base))
        self.identity = (self.root.stat().st_dev, self.root.stat().st_ino)
        self.info.update(created=True, path=str(self.root), deviceAndInode=self.identity,
                         supervisorUid=os.geteuid(), driverUid=self.uid, driverGid=self.gid)
        self.fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        os.fchown(self.fd, os.geteuid(), self.gid)
        os.fchmod(self.fd, 0o710)
        self.work = self.root / 'w'
        self.work.mkdir(mode=0o700)
        os.chown(self.work, self.uid, self.gid)
        self.work_identity = (self.work.stat().st_dev, self.work.stat().st_ino)
        for name in ('h', 't'):
            path = self.work / name
            path.mkdir(mode=0o700)
            os.chown(path, self.uid, self.gid)
        self.home, self.tmp = self.work / 'h', self.work / 't'
        socket_bytes = len(os.fsencode(str(self.tmp) + self.SOCKET_SUFFIX))
        require(socket_bytes <= 100, 'scratch socket path exceeds conservative Unix path budget')
        self.info.update(ready=True, cwd=str(self.work), home=str(self.home), tmp=str(self.tmp),
                         parentMode='0710', workMode='0700', socketPathBudgetBytes=100,
                         reservedSocketPathBytes=socket_bytes)
        self.guard()

    def guard(self):
        require(self.root is not None and self.fd is not None, 'scratch identity unavailable')
        current, opened = self.root.lstat(), os.fstat(self.fd)
        require(stat.S_ISDIR(current.st_mode) and (current.st_dev, current.st_ino) == self.identity
                == (opened.st_dev, opened.st_ino), 'scratch parent identity changed')
        require(current.st_uid == os.geteuid() and current.st_gid == self.gid
                and stat.S_IMODE(current.st_mode) == 0o710, 'scratch parent owner/permissions changed')
        require(os.listdir(self.fd) == ['w'], 'unexpected scratch parent entry')
        work = os.stat('w', dir_fd=self.fd, follow_symlinks=False)
        require(stat.S_ISDIR(work.st_mode) and (work.st_dev, work.st_ino) == self.work_identity
                and work.st_uid == self.uid and work.st_gid == self.gid
                and stat.S_IMODE(work.st_mode) == 0o700, 'scratch work identity/owner/permissions changed')

    def cleanup(self, processes_stopped):
        require(processes_stopped, 'scratch cleanup refused while created processes may remain')
        self.guard()
        count = 0
        # Reject foreign owners/mounts and special devices before deletion. No
        # browser/driver is still running; links are inspected, never traversed.
        for _, directories, files, fd in os.fwalk('w', dir_fd=self.fd, follow_symlinks=False):
            for name in directories + files:
                entry = os.stat(name, dir_fd=fd, follow_symlinks=False)
                count += 1
                require(count <= 100000, 'scratch entry bound exceeded; retained for review')
                require(entry.st_dev == self.identity[0] and entry.st_uid == self.uid,
                        'foreign scratch owner/mount; refusing cleanup')
                require(any(check(entry.st_mode) for check in (stat.S_ISDIR, stat.S_ISREG, stat.S_ISLNK, stat.S_ISSOCK)),
                        'unexpected special scratch entry; refusing cleanup')
        self.guard()
        shutil.rmtree('w', dir_fd=self.fd)
        current = self.root.lstat()
        require((current.st_dev, current.st_ino) == self.identity and not os.listdir(self.fd),
                'scratch parent changed after child cleanup')
        self.root.rmdir()
        self.info['removed'] = True

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


class Enrollment:
    def __init__(self, group, uid, gid):
        self.group, self.uid, self.gid = group, uid, gid
        self.token = secrets.token_hex(32)
        self.root = Path(tempfile.mkdtemp(prefix="ob-memory-socket-", dir="/tmp"))
        self.root.chmod(0o755)
        self.path = self.root / "enroll.sock"
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.bind(str(self.path))
        os.chown(self.path, uid, gid)
        self.path.chmod(0o600)
        self.sock.listen(1)
        self.sock.settimeout(.25)
        self.stopped = threading.Event()
        self.receipt = {}

    def start(self, driver_pid, direct=False):
        def run():
            try:
                deadline = time.monotonic() + 35
                connection = None
                while connection is None and not self.stopped.is_set():
                    require(time.monotonic() < deadline, 'launcher enrollment timeout')
                    try:
                        connection = self.sock.accept()[0]
                    except socket.timeout:
                        pass
                require(connection is not None, 'launcher enrollment stopped')
                with connection as conn:
                    conn.settimeout(10)
                    pid, uid, gid = struct.unpack("3i", conn.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
                    message = json.loads(conn.makefile("rb").readline(4096))
                    require(uid == self.uid and gid == self.gid and message == {"token": self.token}, "unauthorized enrollment")
                    require((direct and pid == driver_pid) or (not direct and pid != driver_pid and descendant(pid, driver_pid)), "not a created browser launcher")
                    before = proc(pid)
                    require(before["uids"] == [uid] * 4 and before["noNewPrivs"] == 1 and all(int(before[key], 16) == 0 for key in ('capEff', 'capPrm', 'capAmb')),
                            "browser launcher privilege restrictions missing")
                    self.group.guard()
                    require(not (self.group.path / "cgroup.procs").read_text().strip(), "group is not fresh at enrollment")
                    (self.group.path / "cgroup.procs").write_text(str(pid))
                    after = proc(pid)
                    require(after["startTicks"] == before["startTicks"] and after["membership"] == self.group.membership,
                            "PID/membership race at enrollment")
                    self.receipt = {"status": "enrolled", "before": before, "after": after,
                                    "launchBeforeBrowserExec": True}
                    conn.sendall((json.dumps({"status": "enrolled", "membership": self.group.membership}) + "\n").encode())
            except Exception as error:
                self.receipt = {"status": "failed", "error": repr(error)}
        self.thread = threading.Thread(target=run, daemon=True)
        self.thread.start()

    def close(self):
        self.stopped.set()
        self.sock.close()
        if hasattr(self, 'thread'):
            self.thread.join(timeout=11)
        # Exact created socket + empty directory only; no recursive deletion.
        self.path.unlink()
        self.root.rmdir()


def run_job(job, b, files, parent, output, uid, gid):
    output.mkdir()
    group = None
    enrollment = None
    child = None
    scratch = None
    receipt = {"job": job, "status": "failed", "errors": [], "scope": "Linux cgroup-v2 kernel charges; no RSS/live-heap claim"}
    try:
        group = Group(parent)
        receipt["group"] = {"path": str(group.path), "deviceAndInode": group.identity, "limits": {"memory.max": CEILING, "memory.swap.max": 0}}
        initial = group.snapshot('created', None)
        require(initial['parsed']['cgroup.procs'] == [] and initial['parsed']['cgroup.events']['populated'] == 0,
                'fresh group unexpectedly populated')
        enrollment = Enrollment(group, uid, gid)
        scratch = Scratch(uid, gid)
        scratch.create()
        work = scratch.work
        direct = job["mode"] == "exited-child-control"
        env = {"PATH": "/usr/bin:/bin", "HOME": str(scratch.home), "TMPDIR": str(scratch.tmp),
               "LANG": "C.UTF-8", "OB_MEMORY_SOCKET": str(enrollment.path), "OB_MEMORY_TOKEN": enrollment.token,
               "OB_MEMORY_EXECUTABLE": b["python"]["path"] if direct else b["browsers"][job["browser"]]["launcher"]["path"],
               "WK_CHECKOUT_PATH": str(work / "does-not-exist-webkit-checkout")}
        if direct:
            command = [b["python"]["path"], str(HERE / "enter.py"), str(HERE / "touch.py")]
        else:
            config = {"job": job, "bindings": b, "packageFiles": files, "wrapper": str(HERE / "enter.py")}
            save(output / "driver-input.json", config)
            command = [b["node"]["path"], str(HERE / "driver.mjs"), str(output / "driver-input.json")]
        receipt["command"] = command
        receipt["scratch"] = scratch.info
        child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 env=env, cwd=work, user=uid, group=gid, extra_groups=[], start_new_session=True)
        receipt["driverIdentity"] = proc(child.pid)
        enrollment.start(child.pid, direct)
        selector = selectors.DefaultSelector()
        selector.register(child.stdout, selectors.EVENT_READ, "stdout")
        selector.register(child.stderr, selectors.EVENT_READ, "stderr")
        deadline = time.monotonic() + (30 if direct else 180)
        line = b""
        seen = set()
        total_bytes = 0
        result = None
        next_poll = time.monotonic() + 1
        with (output / "stdout.jsonl").open("wb") as stdout, (output / "stderr.log").open("wb") as stderr:
            while selector.get_map():
                require(time.monotonic() < deadline, "external job timeout; not SDK cancellation")
                for key, _ in selector.select(.25):
                    block = os.read(key.fileobj.fileno(), 65536)
                    if not block:
                        selector.unregister(key.fileobj)
                        continue
                    (stdout if key.data == "stdout" else stderr).write(block)
                    total_bytes += len(block)
                    require(total_bytes <= 16 * MIB, "job log cap exceeded; preserved prefix and failed witness")
                    if key.data == "stderr":
                        continue
                    line += block
                    require(len(line) <= 2 * MIB, "oversized protocol line")
                    while b"\n" in line:
                        raw, line = line.split(b"\n", 1)
                        message = json.loads(raw)
                        if message["type"] == "phase":
                            row = message["row"]
                            require(row["phase"] not in seen, "duplicate phase")
                            seen.add(row["phase"])
                            snapshot = group.snapshot(row["phase"], row, None if direct else child.pid)
                            if not direct:
                                browser = b["browsers"][job["browser"]]
                                members = [p for p in snapshot["processes"] if "executable" in p]
                                require(any(p["executable"] == browser["actualExecutable"]["path"] for p in members), "pinned browser executable absent from measured group")
                                for p in members:
                                    require(p["uids"] == [uid] * 4 and p["noNewPrivs"] == 1 and all(int(p[key], 16) == 0 for key in ('capEff', 'capPrm', 'capAmb')),
                                            "browser descendant gained privilege")
                                require(sha(browser["actualExecutable"]["path"]) == browser["actualExecutable"]["sha256"], "actual browser bytes changed")
                            child.stdin.write(b"ack\n")
                            child.stdin.flush()
                        elif message["type"] == "result":
                            require(result is None, "duplicate result")
                            result = message
                        else:
                            raise ValueError("unknown driver protocol message")
                if time.monotonic() >= next_poll:
                    group.snapshot("periodic", None, None if direct else child.pid if child.poll() is None else None)
                    next_poll = time.monotonic() + 1
            require(not line, "truncated protocol result")
        receipt["exitCode"] = child.wait(timeout=10)
        require(receipt["exitCode"] == 0 and result is not None, "driver exited without successful complete result")
        group.snapshot("driver-exited", None)
        receipt["result"] = result
        require(enrollment.receipt.get("status") == "enrolled", "launch isolation was not witnessed")
        require(result["status"] == "passed", "driver/Worker failed")
        if direct:
            phases = {r["phase"]: r for r in group.rows}
            require(set(seen) == {"idle", "held-64MiB"}, "child control protocol incomplete")
            idle = phases["idle"]["parsed"]["memory.current"]
            held = phases["held-64MiB"]["parsed"]
            terminal = phases["driver-exited"]["parsed"]
            require(48 * MIB <= held["memory.current"] - idle <= 160 * MIB, "child64 charge growth not detected")
            require(terminal["memory.peak"] >= held["memory.current"] and not terminal["cgroup.procs"]
                    and terminal["cgroup.events"]["populated"] == 0, "child peak did not survive exit/child still present")
            receipt["judgment"] = {"status": "passed", "postExitPeakBytes": terminal["memory.peak"], "physicalHeapCollectionClaim": False}
        receipt["status"] = "observed"
    except Exception as error:
        receipt["errors"].append(repr(error))
    finally:
        if child and child.poll() is None:
            # Only the created Popen process is killed here. Browser descendants
            # are cleaned atomically through our still-owned cgroup below.
            child.kill()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                receipt["errors"].append("created driver failed to exit")
        if enrollment:
            try:
                enrollment.close()
                receipt['enrollmentClosed'] = True
            except Exception as error:
                receipt['enrollmentClosed'] = False
                receipt['errors'].append('enrollment cleanup: ' + repr(error))
            receipt["enrollment"] = enrollment.receipt
        if group:
            try:
                group.cleanup()
                receipt["groupDeleted"] = True
            except Exception as error:
                receipt["errors"].append("cleanup: " + repr(error))
                receipt["groupDeleted"] = getattr(group, 'deleted', False)
            receipt["snapshots"] = group.rows
            if receipt["status"] == "observed" and not receipt["errors"]:
                try:
                    healthy(group.rows)
                    if not direct:
                        receipt["judgment"] = judge(job, group.rows, receipt["result"])
                    receipt["status"] = receipt["judgment"]["status"]
                except Exception as error:
                    receipt["errors"].append("judgment: " + repr(error))
        receipt['driverStopped'] = child is None or child.poll() is not None
        if scratch:
            try:
                scratch.cleanup(receipt['driverStopped'] and receipt.get('groupDeleted', group is None))
            except Exception as error:
                scratch.info['error'] = repr(error)
                receipt['errors'].append('scratch cleanup: ' + repr(error))
            finally:
                scratch.close()
            receipt['scratch'] = scratch.info
        if receipt["errors"]:
            receipt["status"] = "failed"
        receipt['safeToContinue'] = (receipt['driverStopped'] and receipt.get('groupDeleted', group is None)
                                     and receipt.get('enrollmentClosed', enrollment is None)
                                     and (scratch is None or scratch.info['removed']))
        save(output / "RECEIPT.json", receipt)
    return {"id": job["id"], "status": receipt["status"], "judgment": receipt.get("judgment"), "errors": receipt["errors"], "safeToContinue": receipt['safeToContinue']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bindings", type=Path, required=True)
    parser.add_argument("--review", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--uid", type=int, required=True)
    parser.add_argument("--gid", type=int, required=True)
    parser.add_argument("--preflight-only", action="store_true")
    args = parser.parse_args()
    require(args.output.is_absolute() and args.output.parent.resolve() == args.output.parent, "output must have existing canonical absolute parent")
    args.output.mkdir()  # no overwrite, resume or selective rerun
    args.output.chmod(0o755)
    preparation = {"status": "failed", "sdkAttemptConsumed": False}
    parent = None
    parent_identity = None
    parent_rows = []
    phase = 'admission'
    stopped = None
    def sample_parent(label):
        row = {'phase': label, 'raw': {}, 'parsed': None}
        parent_rows.append(row)
        for name in COUNTERS:
            row['raw'][name] = (parent / name).read_text()
        row['parsed'] = parse_snapshot(row['raw'])
        save(args.output / 'PARENT-COUNTERS.json', parent_rows)
    try:
        b, files, identity = bindings(args.bindings, args.review)
        preparation.update(identity)
        preparation["host"] = host(args.uid, args.gid)
        require(isinstance(b['host']['imageVersion'], str) and b['host']['imageVersion'], 'missing frozen GitHub image version')
        require(preparation['host']['runner']['ImageVersion'] == b['host']['imageVersion'], 'GitHub image changed')
        require(preparation['host']['dpkgSha256'] == b['host']['dpkgSha256'], 'host package versions changed')
        preparation["status"] = "passed"
        save(args.output / "PREPARATION.json", preparation)
        save(args.output / 'FROZEN-BINDINGS.json', b)
        if args.preflight_only:
            return 0
        phase = 'control-setup'
        parent = Path("/sys/fs/cgroup") / ("ob-memory-campaign-" + uuid.uuid4().hex)
        parent.mkdir(mode=0o755)
        parent_identity = (parent.stat().st_dev, parent.stat().st_ino)
        (parent / 'memory.max').write_text(str(CEILING))
        (parent / 'memory.swap.max').write_text('0')
        (parent / "cgroup.subtree_control").write_text("+memory")
        sample_parent('created')
        control_jobs = [{"id": "exited-child-control", "mode": "exited-child-control"}] + [
            {"id": name + "-worker-control", "browser": name, "mode": "control"} for name in ("chromium", "webkit")]
        save(args.output / "CONTROL-ATTEMPT.json", {**identity, "jobs": control_jobs})
        controls = []
        phase = 'controls'
        for job in control_jobs:
            if STOP_REQUESTED or controls and not controls[-1]['safeToContinue']:
                controls.append({'id': job['id'], 'status': 'not-executed-interruption-or-unsafe-cleanup', 'safeToContinue': False})
            else:
                controls.append(run_job(job, b, files, parent, args.output / job["id"], args.uid, args.gid))
                sample_parent('after-' + job['id'])
        save(args.output / "CONTROLS.json", {"results": controls, "status": "passed" if all(r["status"] == "passed" for r in controls) else "failed"})
        require(all(row["status"] == "passed" for row in controls), "instrument controls failed; SDK ATTEMPT not consumed")
        healthy(parent_rows)
        # Detect changes during preparation/control work before consuming SDK attempt.
        phase = 'pre-sdk-revalidation'
        _, _, unchanged = bindings(args.bindings, args.review)
        require(unchanged == identity, "inputs changed before SDK ATTEMPT")
        jobs = schedule()
        save(args.output / "ATTEMPT.json", {**identity, "jobs": jobs, "noRetries": True, "trialTimeoutSeconds": 180})
        phase = 'sdk-trials'
        results = []
        for job in jobs:
            if STOP_REQUESTED or results and not results[-1]['safeToContinue']:
                results.append({'id': job['id'], 'status': 'not-executed-interruption-or-unsafe-cleanup', 'safeToContinue': False})
            else:
                results.append(run_job(job, b, files, parent, args.output / job["id"], args.uid, args.gid))
                sample_parent('after-' + job['id'])
            save(args.output / "RESULTS.json", {"complete": False, "results": results})
        status = "passed" if all(r["status"] == "passed" for r in results) else "blocked"
        try:
            healthy(parent_rows)
        except Exception as error:
            status = 'blocked'
            save(args.output / 'PARENT-LIMIT-FAILURE.json', {'error': repr(error)})
        save(args.output / "RESULTS.json", {"complete": len(results) == 42, "status": status, "results": results,
            "driftPolicy": "Any >8MiB released200-to400 trial blocks automatic qualification for attribution; review all three trials per browser for reproducibility. No retry or GC."})
        return 0 if status == "passed" else 1
    except Exception as error:
        stopped = stop_receipt(preparation, args.output, error, phase)
        save(args.output / "STOP.json", stopped)
        return 1
    finally:
        if parent is not None:
            cleanup = {"path": str(parent), "removed": False}
            try:
                require(not parent.is_symlink() and (parent.stat().st_dev, parent.stat().st_ino) == parent_identity, "parent identity changed")
                require(not any(p.is_dir() for p in parent.iterdir()), "unremoved child groups; refusing recursive cleanup")
                sample_parent('terminal-before-delete')
                parent.rmdir()
                cleanup["removed"] = True
            except Exception as error:
                cleanup["error"] = repr(error)
            save(args.output / "PARENT-CLEANUP.json", cleanup)
            if not cleanup['removed']:
                if stopped is None:
                    stopped = stop_receipt(preparation, args.output, RuntimeError(cleanup['error']), 'parent-cleanup')
                stopped['parentCleanupError'] = cleanup['error']
                save(args.output / "STOP.json", stopped)
                return 1


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, stop_requested)
    signal.signal(signal.SIGINT, stop_requested)
    sys.exit(main())
