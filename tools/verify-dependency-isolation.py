"""Audit upstream source identities, the single-serde graph, and SDK boundaries."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import tarfile
import tomllib

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--upstream-dir', type=Path, help='Also verify the original downloaded crate archives')
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
graph = json.loads(subprocess.check_output(
    ['cargo', 'metadata', '--offline', '--locked', '--format-version', '1'], cwd=root))
records = json.loads((root / 'docs/dependency-isolation.json').read_text())['registry_packages']
for record in records:
    matches = [p for p in graph['packages'] if p['name'] == record['package']]
    assert len(matches) == 1, record['package']
    package = matches[0]
    assert package['version'] == record['version'] and package['source'].startswith('registry+'), package
    directory = Path(package['manifest_path']).parent
    actual = {p.relative_to(directory).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in directory.rglob('*.rs') if 'target' not in p.parts}
    assert actual == record['rust_files'], record['package']
    if a.upstream_dir:
        archive = a.upstream_dir / (record['package'] + '-' + record['version'] + '.crate')
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == record['archive_sha256']
        with tarfile.open(archive) as tar:
            original = {m.name.split('/', 1)[1]: hashlib.sha256(tar.extractfile(m).read()).hexdigest()
                        for m in tar.getmembers() if m.isfile() and m.name.endswith('.rs')}
        assert original == record['rust_files']
removed = {'openbindings-internal-' + name for name in ['serde-json', 'referencing', 'jsonschema-value']}
assert not removed & {p['name'] for p in graph['packages']}
assert {p.name for p in (root / 'vendor').iterdir() if p.is_dir()} == {
    'openbindings-internal-jsonschema', 'openbindings-internal-regress'}
tree = subprocess.check_output(['cargo', 'tree', '--offline', '--locked', '--workspace',
    '--edges', 'normal,build', '--no-dedupe', '--prefix', 'none', '--format', '{p}|{f}'], cwd=root, text=True)
serde = {line for line in tree.splitlines() if line.startswith('serde_json v')}
assert len(serde) == 1 and 'arbitrary_precision' not in next(iter(serde)), serde
print('One registry serde_json identity; no SDK-requested arbitrary precision; three upstream source/archives audited; two retained forks.')

# Cancellation has one delivery identity, with no async dependency entering core.
cancellation = 'openbindings-internal-cancellation'
core = tomllib.loads((root / 'crates/openbindings/Cargo.toml').read_text())
assert cancellation not in core.get('dependencies', {})
internal = tomllib.loads((root / 'crates' / cancellation / 'Cargo.toml').read_text())
assert set(internal['dependencies']) == {'openbindings'}
assert internal['dependencies']['openbindings'] == {
    'version': '=0.2.0-alpha.1', 'path': '../openbindings',
}
for role in ['http-discovery', 'invocation', 'source-inspection', 'interface-synthesis']:
    package = root / 'crates' / ('openbindings-' + role)
    manifest = tomllib.loads((package / 'Cargo.toml').read_text())
    assert manifest['dependencies'][cancellation] == {
        'version': '=0.2.0-alpha.1', 'path': '../' + cancellation,
    }
    assert 'pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};' in (package / 'src/lib.rs').read_text()
print('Cancellation: one shared identity, std-only implementation above synchronous core.')

# Core's complete implementation closure must not compile the bounds planner.
graph = json.loads(subprocess.check_output(
    ['cargo', 'metadata', '--offline', '--locked', '--format-version', '1'], cwd=root))
packages = {p['id']: p for p in graph['packages']}
nodes = {n['id']: n for n in graph['resolve']['nodes']}
pending = [next(p['id'] for p in graph['packages'] if p['name'] == 'openbindings')]
closure = set()
while pending:
    node = pending.pop()
    if node in closure:
        continue
    closure.add(node)
    pending.extend(d['pkg'] for d in nodes[node]['deps']
                   if any(k['kind'] is None for k in d['dep_kinds']))
assert 'openbindings-json-schema-evaluator' not in {packages[p]['name'] for p in closure}
for name in ['openbindings', 'openbindings-internal-schema']:
    sources = root / 'crates' / name / 'src'
    for path in sources.rglob('*.rs'):
        assert path.name not in ('bounds.rs', 'oneof.rs'), path
        text = path.read_text()
        assert 'struct EvaluationBounds' not in text and 'struct Plan' not in text, path
internal = tomllib.loads((root / 'crates/openbindings-internal-schema/Cargo.toml').read_text())
assert 'openbindings' not in internal['dependencies']
assert 'openbindings-json-schema-evaluator' not in internal['dependencies']
print('Schema boundary: core has no evaluator dependency or bounds/oneOf planner; the shared adapter has no reverse dependency.')
