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
records = json.loads((root / 'docs/dependency-isolation.json').read_text(encoding='utf-8'))['registry_packages']
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
removed = {'openbindings-internal-' + name
           for name in ['serde-json', 'referencing', 'jsonschema-value', 'jsonschema', 'regress']}
assert not removed & {p['name'] for p in graph['packages']}
assert not (root / 'vendor').exists(), 'the SDK carries no vendored dependency'
tree = subprocess.check_output(['cargo', 'tree', '--offline', '--locked', '--workspace',
    '--edges', 'normal,build', '--no-dedupe', '--prefix', 'none', '--format', '{p}|{f}'], cwd=root, text=True)
serde = {line for line in tree.splitlines() if line.startswith('serde_json v')}
assert len(serde) == 1 and 'arbitrary_precision' not in next(iter(serde)), serde
print('One registry serde_json identity; no SDK-requested arbitrary precision; '
      f'{len(records)} upstream source/archives audited; no forked dependency.')

# Core and the default evaluator use the unmodified library under its own name,
# and no library type crosses the shared crates.
CORE = ['openbindings', 'openbindings-internal-schema', 'openbindings-internal-json']
manifests = {name: tomllib.loads((root / 'crates' / name / 'Cargo.toml').read_text(encoding='utf-8'))
             for name in [p.name for p in (root / 'crates').iterdir() if (p / 'Cargo.toml').is_file()]}
workspace = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))['workspace']
assert 'exclude' not in workspace, workspace.get('exclude')
assert workspace['dependencies']['jsonschema'] == {'version': '=0.58.6', 'default-features': False}, workspace['dependencies']['jsonschema']
for name, manifest in manifests.items():
    for kind in ['dependencies', 'dev-dependencies', 'build-dependencies']:
        for alias, spec in manifest.get(kind, {}).items():
            assert not (isinstance(spec, dict) and 'package' in spec), (name, kind, alias)
    json_dependency = manifest.get('dependencies', {}).get('openbindings-internal-json')
    enabled = isinstance(json_dependency, dict) and 'evaluator-work' in json_dependency.get('features', [])
    assert enabled == (name == 'openbindings-json-schema-evaluator'), name
    if 'jsonschema' in manifest.get('dependencies', {}):
        assert manifest['dependencies']['jsonschema'] == {'workspace': True}, name
assert manifests['openbindings-internal-json']['features'] == {'evaluator-work': []}
assert manifests['openbindings-json-schema-evaluator']['dependencies']['jsonschema'] == {'workspace': True}
for name in ['openbindings-internal-schema', 'openbindings-internal-json']:
    for path in (root / 'crates' / name / 'src').rglob('*.rs'):
        for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
            code = line.strip()
            if code.startswith(('pub fn', 'pub use', 'pub type', 'pub struct', 'pub enum', 'pub trait')):
                for library in ['jsonschema', 'referencing', 'Validator', 'ValidationOptions', 'Registry', 'Keyword<']:
                    assert library not in code, (path, number, code)
print('Core and the default evaluator: unmodified jsonschema only, and no library type crosses.')

# Cancellation has one delivery identity, with no async dependency entering core.
cancellation = 'openbindings-internal-cancellation'
core = tomllib.loads((root / 'crates/openbindings/Cargo.toml').read_text(encoding='utf-8'))
assert cancellation not in core.get('dependencies', {})
internal = tomllib.loads((root / 'crates' / cancellation / 'Cargo.toml').read_text(encoding='utf-8'))
assert set(internal['dependencies']) == {'openbindings'}
assert internal['dependencies']['openbindings'] == {
    'version': '=0.2.0-alpha.1', 'path': '../openbindings',
}
for interface in ['http-discovery', 'invocation', 'source-inspection', 'interface-synthesis']:
    package = root / 'crates' / ('openbindings-' + interface)
    manifest = tomllib.loads((package / 'Cargo.toml').read_text(encoding='utf-8'))
    assert manifest['dependencies'][cancellation] == {
        'version': '=0.2.0-alpha.1', 'path': '../' + cancellation,
    }
    assert 'pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};' in (package / 'src/lib.rs').read_text(encoding='utf-8')
print('Cancellation: one shared identity, std-only implementation above synchronous core.')

# The interface records the kind-facing companions share have one definition,
# outside core, reexported by each of them.
interfaces = 'openbindings-internal-interfaces'
assert interfaces not in core.get('dependencies', {})
for interface in ['invocation', 'source-inspection', 'interface-synthesis']:
    package = root / 'crates' / ('openbindings-' + interface)
    manifest = tomllib.loads((package / 'Cargo.toml').read_text(encoding='utf-8'))
    assert manifest['dependencies'][interfaces] == {
        'version': '=0.2.0-alpha.1', 'path': '../' + interfaces,
    }
    assert 'pub use openbindings_internal_interfaces::{KindSupportVerdict, SupportedKind};' in ''.join(
        path.read_text(encoding='utf-8') for path in (package / 'src').rglob('*.rs'))
print('Interface records: one shared definition of the kind support records, outside core.')

# Core's complete implementation closure holds no evaluator.
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
internal = tomllib.loads((root / 'crates/openbindings-internal-schema/Cargo.toml').read_text(encoding='utf-8'))
assert 'openbindings' not in internal['dependencies']
assert 'openbindings-json-schema-evaluator' not in internal['dependencies']
print('Schema boundary: core has no evaluator dependency; the shared adapter has no reverse dependency.')
