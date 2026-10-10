"""Exercise an external Cargo consumer and inspect its actual resolved features.

The default replays the checked-in lock. --resolution minimum pins the supported
direct shared dependency floors; latest asks Cargo for its newest resolvable
graph. CARGO_NET_OFFLINE controls cache-only runs. Neither mode claims to test
the minimum version of every transitive dependency or future releases.
"""
from pathlib import Path, PurePosixPath, PureWindowsPath
import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
import tomllib

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--resolution', choices=['locked', 'minimum', 'latest'], default='locked')
p.add_argument('--output', type=Path, help='Retain metadata and test receipts here')
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
source = root / 'tools/consumer-compat'
env = os.environ.copy()
env.setdefault('CARGO_TARGET_DIR', str(root / 'target'))
env.setdefault('CARGO_BUILD_JOBS', '3')
output = a.output.resolve() if a.output else None
if output:
    output.mkdir(parents=True, exist_ok=True)

def run(args, cwd):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    result = subprocess.run(list(map(str, args)), cwd=cwd, env=env, text=True, encoding='utf-8',
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        print(result.stdout)
        print(result.stderr)
        raise SystemExit(result.returncode)
    return result

def consumer_manifest(template, checkout):
    # These are TOML string values, not shell paths. Preserve quotes/Unicode and
    # serialize Windows separators safely instead of interpolating raw text.
    return re.sub(
        r'path = "\.\./\.\./(crates|vendor)/([^"\n]+)"',
        lambda match: 'path = ' + json.dumps(
            (checkout / match[1] / match[2]).as_posix(), ensure_ascii=False),
        template,
    )


# Portable regression controls for the manifest consumed by actual Cargo below.
# In particular, D:\a is not a valid unescaped TOML basic string.
for checkout in [PureWindowsPath(r'D:\a\sdk\sdk'),
                 PureWindowsPath('C:/Users/Ünicode/sdk'),
                 PurePosixPath('/tmp/quote"and\\slash/sdk')]:
    template = '[dependencies]\nx = { path = "../../crates/x" }\ny = { path = "../../vendor/y" }\n'
    parsed = tomllib.loads(consumer_manifest(template, checkout))
    assert parsed['dependencies']['x']['path'] == (checkout / 'crates/x').as_posix()
    assert parsed['dependencies']['y']['path'] == (checkout / 'vendor/y').as_posix()


with tempfile.TemporaryDirectory(prefix='openbindings-consumer-') as temporary:
    fixture = Path(temporary)
    shutil.copytree(source / 'src', fixture / 'src')
    text = (source / 'Cargo.toml').read_text(encoding='utf-8')
    text = consumer_manifest(text, root)
    tomllib.loads(text)  # Refuse malformed generated configuration before Cargo.
    (fixture / 'Cargo.toml').write_text(text, encoding='utf-8')
    shutil.copyfile(source / 'Cargo.lock', fixture / 'Cargo.lock')
    floors = {'serde': '1.0.229', 'serde_json': '1.0.151', 'itoa': '1.0.18',
              'fluent-uri': '0.4.1', 'reqwest': '0.13.5', 'tokio': '1.53.2', 'bytes': '1.12.1'}
    if a.resolution == 'minimum':
        for package, version in floors.items():
            run(['cargo', 'update', '-p', package, '--precise', version], fixture)
    elif a.resolution == 'latest':
        run(['cargo', 'update'], fixture)
    if output:
        shutil.copyfile(fixture / 'Cargo.lock', output / 'Cargo.lock')
    host = next(line.removeprefix('host: ') for line in run(['rustc', '-vV'], fixture).stdout.splitlines()
                if line.startswith('host: '))
    receipts = []
    baseline = None
    for features in ['', 'ap', 'sdk', 'sdk,ap', 'evaluator', 'native', 'native,ap']:
        label = features.replace(',', '-') or 'baseline'
        selection = ['--no-default-features'] + (['--features', features] if features else [])
        metadata = json.loads(run(['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', host, *selection], fixture).stdout)
        # metadata includes weak optional feature edges (such as the unused
        # jsonschema macros). tree describes the actual host build selection.
        tree = run(['cargo', 'tree', '--locked', '--prefix', 'none', '--no-dedupe',
                    '--edges', 'normal,build', '--format', '{p}|{f}', *selection], fixture).stdout
        nodes = {}
        for line in tree.splitlines():
            identity, feature_text = line.split('|', 1)
            name, version_path = identity.split(' v', 1)
            nodes[name] = {'identity': identity, 'version': version_path.split(' ', 1)[0],
                           'features': feature_text.split(',') if feature_text else []}
        shared = nodes['serde_json']
        private = nodes.get('openbindings-internal-serde-json')
        shared_features = sorted(shared['features'])
        if not features:
            baseline = shared_features
        expected = set(baseline)
        if 'ap' in features.split(','):
            expected.add('arbitrary_precision')
        if features not in ['', 'ap']:
            expected.add('raw_value')  # Preserve the public JsonValue Serde protocol.
        assert shared_features == sorted(expected), (label, shared_features)
        if features not in ['', 'ap']:
            assert private is not None and 'arbitrary_precision' in private['features'], label
            assert private['identity'] != shared['identity'], label
            assert 'openbindings-internal-referencing' in nodes, label
            assert 'referencing' not in nodes, label
        else:
            assert private is None, label
        result = run(['cargo', 'test', '--locked', *selection], fixture)
        row = {'configuration': label, 'serde_json_features': shared_features,
               'internal_json_features': sorted(private['features']) if private else [],
               'direct_shared_versions': {name: nodes[name]['version']
                                          for name in floors if name in nodes}, 'tests': 'passed'}
        if a.resolution == 'minimum':
            assert all(version == floors[name] for name, version in row['direct_shared_versions'].items()), row
        receipts.append(row)
        if output:
            (output / (label + '-metadata.json')).write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
            (output / (label + '-features.txt')).write_text(tree, encoding='utf-8')
            (output / (label + '-tests.log')).write_text(result.stdout + result.stderr, encoding='utf-8')
        print(json.dumps(row), flush=True)
    summary = {'resolution': a.resolution, 'host': host, 'offline': env.get('CARGO_NET_OFFLINE', 'false'), 'configurations': receipts}
    if output:
        (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n', encoding='utf-8')
