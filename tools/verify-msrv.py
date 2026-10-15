"""Assert that owned manifests and Clippy use the independently selected MSRV."""
from pathlib import Path
import argparse
import os
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def check(version):
    version = version.removesuffix(".0") if version.count(".") == 2 else version
    workspace = tomllib.loads((ROOT / 'Cargo.toml').read_text())
    assert workspace['workspace']['package']['rust-version'] == version, 'workspace MSRV differs from lane'
    assert tomllib.loads((ROOT / 'clippy.toml').read_text())['msrv'] == version, 'Clippy MSRV differs from lane'
    manifests = []
    for base in ['crates', 'vendor', 'examples', 'tools']:
        for directory, children, files in os.walk(ROOT / base):
            children[:] = [name for name in children if name not in {'target', 'node_modules', '.git'}]
            if 'Cargo.toml' in files:
                manifests.append(Path(directory) / 'Cargo.toml')
    for manifest in manifests:
        package = tomllib.loads(manifest.read_text())['package']
        declared = package.get('rust-version')
        expected = {'workspace': True} if manifest.parent.parent.name == 'crates' else version
        assert declared == expected, f'{manifest.relative_to(ROOT)}: inconsistent rust-version {declared!r}'
    print(f'MSRV {version}: workspace, Clippy and {len(manifests)} packages agree')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True, help='Compiler version selected by the CI lane')
    check(parser.parse_args().version)
