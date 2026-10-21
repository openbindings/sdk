"""Assert that owned manifests and Clippy use the independently selected MSRV."""
from pathlib import Path
import argparse
import json
import os
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def test_workspace(version):
    inventory = json.loads((ROOT / 'tools/stack-margin-tests.json').read_text(encoding='utf-8'))
    skipped = []
    for entry in inventory['tests']:
        assert (ROOT / entry['source']).is_file(), entry['source']
        assert entry['reason'], entry['source']
        if entry['workspace']:
            skipped.extend(entry['names'])
            print(f"MSRV stack exclusion: {entry['names']}: {entry['reason']}", flush=True)
    assert skipped and len(skipped) == len(set(skipped)), 'empty or duplicate stack exclusions'
    command = ['cargo', f'+{version}', 'test', '--workspace', '--all-features', '--locked', '--no-fail-fast']
    # Fail if an exclusion no longer names exactly one real workspace test.
    listing = subprocess.run([*command, '--', '--list'], cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE)
    names = [line.removesuffix(': test') for line in listing.stdout.splitlines() if line.endswith(': test')]
    for name in skipped:
        assert names.count(name) == 1, f'stale or ambiguous stack exclusion: {name}'
    # libtest applies --exact to --skip too; related semantic/depth tests stay on.
    command.extend(['--', '--exact'])
    for name in skipped:
        command.extend(['--skip', name])
    print('+ ' + ' '.join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def check(version):
    version = version.removesuffix(".0") if version.count(".") == 2 else version
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))
    assert workspace['workspace']['package']['rust-version'] == version, 'workspace MSRV differs from lane'
    assert tomllib.loads((ROOT / 'clippy.toml').read_text(encoding='utf-8'))['msrv'] == version, 'Clippy MSRV differs from lane'
    manifests = []
    for base in ['crates', 'examples', 'tools']:
        for directory, children, files in os.walk(ROOT / base):
            children[:] = [name for name in children if name not in {'target', 'node_modules', '.git'}]
            if 'Cargo.toml' in files:
                manifests.append(Path(directory) / 'Cargo.toml')
    for manifest in manifests:
        package = tomllib.loads(manifest.read_text(encoding='utf-8'))['package']
        declared = package.get('rust-version')
        expected = {'workspace': True} if manifest.parent.parent.name == 'crates' else version
        assert declared == expected, f'{manifest.relative_to(ROOT)}: inconsistent rust-version {declared!r}'
    print(f'MSRV {version}: workspace, Clippy and {len(manifests)} packages agree')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True, help='Compiler version selected by the CI lane')
    parser.add_argument('--test', action='store_true', help='Run the MSRV workspace tests with exact stack-margin exclusions')
    args = parser.parse_args()
    check(args.version)
    if args.test:
        test_workspace(args.version)
