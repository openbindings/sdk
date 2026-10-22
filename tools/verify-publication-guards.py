"""Require the guards that keep every SDK package unpublished.

Every workspace package keeps Cargo's `publish = false`, and the npm package keeps
`"private": true`, until an authorized release decision. That release change edits
the guards and narrows this check to the packages it approves; nothing here publishes.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
NPM_MANIFEST = ROOT / 'packages/typescript/package.json'


def validate(packages, npm_manifest):
    """Check `cargo metadata` packages and the npm manifest; return the guarded names."""
    if not packages:
        raise ValueError('No workspace packages to check')
    for package in packages:
        # Cargo reports `publish = false` as an empty registry list; absence
        # (null) or any named registry would allow publication.
        if package.get('publish') != []:
            raise ValueError(f'{package["name"]} must keep publish = false')
    if npm_manifest.get('private') is not True:
        raise ValueError(f'{npm_manifest.get("name")} must keep "private": true')
    return sorted(package['name'] for package in packages) + [npm_manifest['name']]


def self_test():
    guarded = {'name': 'openbindings', 'publish': []}
    npm = {'name': '@openbindings/sdk', 'private': True}
    assert validate([guarded], npm) == ['openbindings', '@openbindings/sdk']
    invalid = [
        ([], npm),
        ([{'name': 'openbindings', 'publish': None}], npm),
        ([{'name': 'openbindings'}], npm),
        ([{'name': 'openbindings', 'publish': ['crates-io']}], npm),
        ([guarded], {'name': '@openbindings/sdk'}),
        ([guarded], {'name': '@openbindings/sdk', 'private': False}),
        ([guarded], {'name': '@openbindings/sdk', 'private': 'true'}),
    ]
    for packages, manifest in invalid:
        try:
            validate(packages, manifest)
        except ValueError:
            continue
        raise AssertionError(f'Accepted a publishable package: {packages!r} {manifest!r}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        self_test()
    metadata = json.loads(subprocess.run(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'],
        cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE).stdout)
    guarded = validate(metadata['packages'], json.loads(NPM_MANIFEST.read_text(encoding='utf-8')))
    print(f'Publication guards: {len(guarded)} packages unpublished ({", ".join(guarded)})')


if __name__ == '__main__':
    main()
