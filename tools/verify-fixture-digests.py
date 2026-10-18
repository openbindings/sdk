"""Require every vendored interface fixture to match the digest its README records.

Each `crates/*/tests/fixtures/README.md` table lists the fixture files beside it
with their SHA-256 digests. A file whose bytes drift from its row, a row naming a
missing file, or a fixture file without a row fails the check.
"""
import argparse
import hashlib
from pathlib import Path
import re
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ROW = re.compile(r'^\|\s*`([^`]+)`\s*\|[^|]*\|\s*`([0-9a-f]{64})`\s*\|\s*$')


def rows(readme):
    recorded = {}
    for line in readme.read_text(encoding='utf-8').splitlines():
        match = ROW.match(line)
        if match:
            path, digest = match.groups()
            if path in recorded:
                raise ValueError(f'{readme}: {path} has more than one row')
            recorded[path] = digest
    if not recorded:
        raise ValueError(f'{readme}: no fixture digest rows')
    return recorded


def check(directory):
    readme = directory / 'README.md'
    recorded = rows(readme)
    present = {
        path.relative_to(directory).as_posix()
        for path in directory.rglob('*')
        if path.is_file() and path != readme
    }
    problems = []
    for path in sorted(present - recorded.keys()):
        problems.append(f'{directory / path}: no digest row in {readme.name}')
    for path, digest in sorted(recorded.items()):
        file = directory / path
        if path not in present:
            problems.append(f'{readme}: {path} names no fixture file')
        elif hashlib.sha256(file.read_bytes()).hexdigest() != digest:
            problems.append(f'{file}: digest differs from its {readme.name} row')
    return problems, len(recorded)


def self_test():
    with tempfile.TemporaryDirectory() as scratch:
        directory = Path(scratch)
        (directory / 'nested').mkdir()
        (directory / 'nested/cases.json').write_bytes(b'{"cases":[]}\n')
        digest = hashlib.sha256(b'{"cases":[]}\n').hexdigest()
        readme = directory / 'README.md'
        table = '| Fixture | Evidence owner | SHA-256 |\n| --- | --- | --- |\n'
        readme.write_text(table + f'| `nested/cases.json` | Pinned | `{digest}` |\n')
        assert check(directory)[0] == []
        (directory / 'nested/cases.json').write_bytes(b'{"cases":[]}\r\n')
        assert check(directory)[0], 'accepted a drifted fixture'
        (directory / 'nested/cases.json').write_bytes(b'{"cases":[]}\n')
        (directory / 'extra.json').write_bytes(b'{}')
        assert check(directory)[0], 'accepted a fixture without a row'
        (directory / 'extra.json').unlink()
        (directory / 'nested/cases.json').unlink()
        assert check(directory)[0], 'accepted a row naming no file'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        self_test()
    problems, checked = [], 0
    for readme in sorted(ROOT.glob('crates/*/tests/fixtures/README.md')):
        found, count = check(readme.parent)
        problems += found
        checked += count
    if problems:
        raise SystemExit('\n'.join(problems))
    print(f'Fixture digests: {checked} vendored files match their README rows')


if __name__ == '__main__':
    main()
