"""Compare internal forks with exact downloaded upstream crate archives."""
from pathlib import Path
import argparse
import difflib
import hashlib
import json
import tarfile

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = [
    ('jsonschema', '0.58.6'), ('jsonschema-value', '0.58.6'),
    ('regress', '0.12.0'), ('serde_json', '1.0.151'), ('referencing', '0.58.6'),
]


def digest(value):
    return hashlib.sha256(value).hexdigest() if value is not None else None


def source_lines(data):
    # Unified patch recognizes LF, not every Unicode separator str.splitlines does.
    pieces = data.decode().split('\n')
    return [part + '\n' for part in pieces[:-1]] + ([pieces[-1]] if pieces[-1] else [])


def unified_patch(file, before, after):
    """Emit an applicable diff, including explicit missing-final-newline markers."""
    try:
        lines = difflib.unified_diff(
            source_lines(before or b''),
            source_lines(after or b''),
            fromfile='a/' + file if before is not None else '/dev/null',
            tofile='b/' + file if after is not None else '/dev/null',
        )
        return ''.join(line if line.endswith('\n') else line + '\n\\ No newline at end of file\n'
                       for line in lines)
    except UnicodeError as error:
        raise ValueError(f'{file}: binary change needs an explicit reconstruction mechanism') from error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--upstream-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    records = []
    for name, version in UPSTREAM:
        package = 'openbindings-internal-' + name.replace('_', '-')
        archive = args.upstream_dir / f'{name}-{version}.crate'
        modified = ROOT / 'vendor' / package
        with tarfile.open(archive) as tar:
            original = {m.name.split('/', 1)[1]: tar.extractfile(m).read()
                        for m in tar.getmembers() if m.isfile()}
        current = {f.relative_to(modified).as_posix(): f.read_bytes()
                   for f in modified.rglob('*')
                   if f.is_file() and 'target' not in f.parts and '.git' not in f.parts}
        changes, patches = [], []
        for file in sorted(original.keys() | current.keys()):
            before, after = original.get(file), current.get(file)
            if before == after:
                continue
            changes.append({'file': file, 'upstream_sha256': digest(before),
                            'current_sha256': digest(after)})
            patches.append(unified_patch(file, before, after))
        (args.output / (name + '.patch')).write_text(''.join(patches))
        records.append({
            'upstream': name, 'version': version,
            'upstream_archive_sha256': digest(archive.read_bytes()),
            'vcs': json.loads(original.get('.cargo_vcs_info.json', b'{}')),
            'modified_package': package, 'changes': changes, 'patch': name + '.patch',
        })
    (args.output / 'manifest.json').write_text(json.dumps({'records': records}, indent=2) + '\n')
    print(json.dumps([{'name': r['upstream'], 'changed_files': len(r['changes'])} for r in records]))


if __name__ == '__main__':
    main()
