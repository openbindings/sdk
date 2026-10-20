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
removed = {'openbindings-internal-' + name for name in ['serde-json', 'referencing', 'jsonschema-value']}
assert not removed & {p['name'] for p in graph['packages']}
assert {p.name for p in (root / 'vendor').iterdir() if p.is_dir()} == {
    'openbindings-internal-jsonschema', 'openbindings-internal-regress'}
tree = subprocess.check_output(['cargo', 'tree', '--offline', '--locked', '--workspace',
    '--edges', 'normal,build', '--no-dedupe', '--prefix', 'none', '--format', '{p}|{f}'], cwd=root, text=True)
serde = {line for line in tree.splitlines() if line.startswith('serde_json v')}
assert len(serde) == 1 and 'arbitrary_precision' not in next(iter(serde)), serde
print('One registry serde_json identity; no SDK-requested arbitrary precision; '
      f'{len(records)} upstream source/archives audited; two retained forks.')

# Core uses the unmodified library; the forks serve only the default evaluator
# until it moves too. Core's own graph, resolved as Cargo builds core alone,
# holds neither fork, and no library type crosses between the two.
FORKS = {'openbindings-internal-jsonschema', 'openbindings-internal-regress'}
CORE = ['openbindings', 'openbindings-internal-schema', 'openbindings-internal-json']
for package in CORE:
    core_tree = subprocess.check_output(['cargo', 'tree', '--offline', '--locked', '-p', package,
        '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], cwd=root, text=True)
    names = {line.split(' v', 1)[0] for line in core_tree.splitlines() if line.strip()}
    assert not FORKS & names, (package, FORKS & names)
    if package != 'openbindings-internal-json':
        assert 'jsonschema' in names, package
manifests = {name: tomllib.loads((root / 'crates' / name / 'Cargo.toml').read_text(encoding='utf-8'))
             for name in [p.name for p in (root / 'crates').iterdir() if (p / 'Cargo.toml').is_file()]}
workspace = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))['workspace']['dependencies']
assert workspace['jsonschema'] == {'version': '=0.58.6', 'default-features': False}, workspace['jsonschema']
def fork_users(name, manifest):
    found = []
    for kind in ['dependencies', 'dev-dependencies', 'build-dependencies']:
        for alias, spec in manifest.get(kind, {}).items():
            if isinstance(spec, dict) and spec.get('package', alias) in FORKS:
                found.append((kind, alias, spec))
    return found
users = {name: fork_users(name, manifest) for name, manifest in manifests.items()}
users = {name: found for name, found in users.items() if found}
assert set(users) == {'openbindings-json-schema-evaluator', 'openbindings-internal-json'}, users
assert [(k, a) for k, a, _ in users['openbindings-json-schema-evaluator']] == [('dependencies', 'jsonschema')]
assert [(k, a, s.get('optional')) for k, a, s in users['openbindings-internal-json']] == [
    ('dependencies', 'evaluator-jsonschema', True)]
assert manifests['openbindings-internal-json']['features'] == {'evaluator-work': ['dep:evaluator-jsonschema']}
for name, manifest in manifests.items():
    json_dependency = manifest.get('dependencies', {}).get('openbindings-internal-json')
    enabled = isinstance(json_dependency, dict) and 'evaluator-work' in json_dependency.get('features', [])
    assert enabled == (name == 'openbindings-json-schema-evaluator'), name
    if name in CORE:
        assert manifest.get('dependencies', {}).get('jsonschema', {'workspace': True}) == {'workspace': True}, name
# The shared crates expose no library type, so the evaluator's schema space and
# the fork never receive one of core's library values, nor core one of theirs.
for name in ['openbindings-internal-schema', 'openbindings-internal-json']:
    for path in (root / 'crates' / name / 'src').rglob('*.rs'):
        for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
            code = line.strip()
            if code.startswith(('pub fn', 'pub use', 'pub type', 'pub struct', 'pub enum', 'pub trait')):
                for library in ['jsonschema', 'referencing', 'Validator', 'ValidationOptions', 'Registry', 'Keyword<']:
                    assert library not in code, (path, number, code)
    for path in (root / 'crates' / name / 'src').rglob('*.rs'):
        text = path.read_text(encoding='utf-8')
        assert 'ob_ecma' not in text, path
        if name == 'openbindings-internal-schema':
            assert 'ob_work' not in text and 'evaluator_jsonschema' not in text, path
print('Core graph: unmodified jsonschema only; the forks reach only the default evaluator, and no library type crosses.')

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
        text = path.read_text(encoding='utf-8')
        assert 'struct EvaluationBounds' not in text and 'struct Plan' not in text, path
internal = tomllib.loads((root / 'crates/openbindings-internal-schema/Cargo.toml').read_text(encoding='utf-8'))
assert 'openbindings' not in internal['dependencies']
assert 'openbindings-json-schema-evaluator' not in internal['dependencies']
print('Schema boundary: core has no evaluator dependency or bounds/oneOf planner; the shared adapter has no reverse dependency.')
