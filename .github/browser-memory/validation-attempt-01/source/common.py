"""Preparation-only helpers. No SDK imports, browser launches or campaign calls."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import signal
import subprocess
import tarfile

HERE = Path(__file__).resolve().parent
CANONICAL = Path('/opt/ob-memory-inputs')
HARNESS_FREEZE = '361ee880e6def05e826ecef1f800a4347b59de2e70ac954da7859cb0ff858825'


def require(value, message):
    if not value: raise ValueError(message)


def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as file:
        for block in iter(lambda: file.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def digest(value, length=64):
    require(isinstance(value, str) and re.fullmatch('[0-9a-f]{' + str(length) + '}', value), 'missing/malformed fixed digest')
    return value


def validate_request(request, mode='prepare'):
    require(request['schemaVersion'] == 1 and request['mode'] == mode, 'wrong request phase')
    require(request['ownerLogin'] and re.fullmatch('[A-Za-z0-9-]+', request['ownerLogin']), 'explicit root actor login required')
    require(request['sdkRepository'] == 'openbindings/sdk', 'SDK repository scope changed')
    for label in ('baseline', 'candidate'):
        row = request[label]
        digest(row['commit'], 40); digest(row['tree'], 40)
        digest(row['expectedRustSourceSha256'])
        if row['expectedArchiveSha256'] is not None: digest(row['expectedArchiveSha256'])
    require(request['baseline']['referenceArchiveSha256'] == '80bceb3e1c71b5c6d6366c7aab5f8aae4481aed513df1cb1f4cdb3a496148994', 'original baseline reference changed')
    require(request['baseline']['tree'] == '86ca4a150675ef4ea286c726409d98e6f18c1633', 'baseline source tree changed')
    require(request['baseline']['expectedRustSourceSha256'] == '32a94299a2adfe9c14288a29cc8584777bba27002192d6ddcbb961f46ce712f6', 'baseline compiler inputs changed')
    require(request['node']['version'] == '22.19.0', 'prospective Node version differs')
    digest(request['node']['archiveSha256'])
    require(request['playwright']['version'] == '1.64.0', 'prospective Playwright version differs')
    digest(request['playwright']['archiveSha256'])
    require(request['rustToolchain'] == '1.99.0' and request['wasmBindgen'] == '0.2.129', 'build toolchain differs')
    require(request['canonicalRoot'] == str(CANONICAL), 'canonical root differs')
    return request


def safe_extract(archive_path, destination, expected_top=None, max_bytes=3 * 1024**3):
    """Validate the complete tar before extracting; no hardlinks/devices/path repair.

    Allow only safe in-tree relative symlinks, with no member beneath a symlink.
    Extract into a fresh directory as the unprivileged runner, never as root.
    """
    destination = Path(destination)
    require(not destination.exists(), 'extraction destination already exists')
    with tarfile.open(archive_path, 'r:*') as archive:
        members = archive.getmembers()
        require(len(members) <= 100000, 'archive member limit exceeded')
        names, links, total = set(), set(), 0
        for member in members:
            name = PurePosixPath(member.name)
            require(member.name and not name.is_absolute() and '..' not in name.parts and str(name) == member.name.rstrip('/'), 'unsafe/noncanonical archive path')
            require(name.parts and name.parts[0] != '.', 'empty archive root')
            if expected_top: require(name.parts[0] == expected_top, 'unexpected archive top directory')
            require(str(name) not in names, 'duplicate archive member')
            names.add(str(name))
            require(member.isfile() or member.isdir() or member.issym(), 'special/hardlink archive member')
            total += member.size
            require(total <= max_bytes and 0 <= member.size <= 1024**3, 'archive size bound exceeded')
            require(member.mode & 0o6000 == 0, 'set-id archive mode')
            if member.issym():
                target = PurePosixPath(member.linkname)
                require(not target.is_absolute(), 'absolute archive symlink')
                resolved = []
                for part in (*name.parent.parts, *target.parts):
                    if part == '..':
                        require(resolved, 'escaping archive symlink')
                        resolved.pop()
                    elif part != '.': resolved.append(part)
                require(resolved and (not expected_top or resolved[0] == expected_top), 'symlink leaves archive root')
                links.add(str(name))
        for name in names:
            require(not any(str(parent) in links for parent in PurePosixPath(name).parents), 'archive member below symlink')
        destination.mkdir(mode=0o755)
        archive.extractall(destination, filter='data')


def tree(root):
    """Same canonical content record as the accepted harness identity.tree()."""
    root = Path(root).resolve(strict=True)
    rows = []
    for path in sorted(root.rglob('*')):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            require(path.resolve(strict=True).is_relative_to(root), 'tree symlink escapes')
            rows.append({'path': relative, 'link': os.readlink(path)})
        elif path.is_file():
            rows.append({'path': relative, 'bytes': path.stat().st_size, 'sha256': sha(path), 'executable': bool(path.stat().st_mode & 0o111)})
        else: require(path.is_dir(), 'special tree entry')
    require(rows, 'empty tree')
    return hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode()).hexdigest(), rows


def file_binding(path):
    path = Path(path).resolve(strict=True)
    return {'path': str(path), 'sha256': sha(path)}


def check_seed(seed):
    seed = Path(seed)
    harness = seed / 'harness'
    require(sha(harness / 'SOURCE-FREEZE.json') == HARNESS_FREEZE, 'accepted harness freeze changed')
    freeze = json.loads((harness / 'SOURCE-FREEZE.json').read_text())
    for name, expected in freeze['files'].items():
        require(sha(harness / name) == expected, 'accepted harness source changed: ' + name)
    review = json.loads((harness / 'OWNER-SOURCE-REVIEW.json').read_text())
    require(review['sourceFreezeSha256'] == HARNESS_FREEZE and review['independentTests']['exitCode'] == 0, 'source acceptance missing')
    validation = freeze['validationReceipt']
    require(sha(harness / validation['path']) == validation['sha256'], 'SDK-free validation receipt changed')
    profiles = json.loads((harness / 'PROFILES.json').read_text())
    for name, profile in profiles.items():
        require(sha(seed / 'fixtures' / (name + '.json')) == profile['sha256'], 'registered fixture changed: ' + name)
    return freeze


def check_preparer(root=HERE):
    root = Path(root)
    freeze = json.loads((root / 'PREPARATION-SOURCE-FREEZE.json').read_text())
    for name, expected in freeze['files'].items():
        path = root / name
        require(path.resolve(strict=True).is_relative_to(root.resolve()), 'preparer file escapes root')
        require(sha(path) == expected, 'reviewed preparation source changed: ' + name)
    return sha(root / 'PREPARATION-SOURCE-FREEZE.json')


def bounded(command, *, timeout, privileged_children=False, **kwargs):
    """Kill only this freshly created session's process group on timeout.

    The direct child remains unreaped until the signal, retaining its PID. No
    name-based cleanup or shared process/cgroup cleanup is permitted.
    """
    process = subprocess.Popen(command, start_new_session=True, **kwargs)
    try:
        return subprocess.CompletedProcess(command, process.wait(timeout=timeout))
    except subprocess.TimeoutExpired:
        require(os.getpgid(process.pid) == process.pid, 'created child group identity differs; refuse cleanup')
        if privileged_children:
            subprocess.run(['sudo', '-n', '/usr/bin/kill', '-KILL', '--', '-' + str(process.pid)],
                           check=True, timeout=10)
        else:
            os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=10)
        raise
