"""Keep the two package-identity forks mechanical, and their private graph wired."""
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
records = json.loads((root / 'docs/dependency-isolation.json').read_text())['mechanical_packages']
for record in records:
    package = root / 'vendor' / record['package']
    actual = {path.relative_to(package).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
              for path in package.rglob('*.rs') if 'target' not in path.parts}
    assert actual == record['rust_files'], f"upstream Rust source changed: {record['package']}"
    if a.upstream_dir:
        archive = a.upstream_dir / (record['upstream'] + '-' + record['version'] + '.crate')
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == record['archive_sha256']
        with tarfile.open(archive) as tar:
            upstream = {member.name.split('/', 1)[1]: hashlib.sha256(tar.extractfile(member).read()).hexdigest()
                        for member in tar.getmembers() if member.isfile() and member.name.endswith('.rs')}
        assert upstream == record['rust_files'], record['package']
    manifest = tomllib.loads((package / 'Cargo.toml').read_text())
    assert manifest['package']['name'] == record['package']
    assert manifest['package']['publish'] is False

for name in ['jsonschema', 'jsonschema-value', 'referencing']:
    manifest = tomllib.loads((root / 'vendor' / ('openbindings-internal-' + name) / 'Cargo.toml').read_text())
    dep = manifest['dependencies']['serde_json']
    assert dep['package'] == 'openbindings-internal-serde-json'
    assert dep['version'] == '=1.0.151-ob.1'
    assert dep['path'] == '../openbindings-internal-serde-json'
manifest = tomllib.loads((root / 'vendor/openbindings-internal-jsonschema/Cargo.toml').read_text())
assert manifest['dependencies']['referencing'] == {
    'package': 'openbindings-internal-referencing', 'path': '../openbindings-internal-referencing',
    'version': '=0.58.6-ob.1',
}
manifest = tomllib.loads((root / 'Cargo.toml').read_text())
bridge = manifest['workspace']['dependencies']['serde_json_compat']
assert bridge == {'package': 'serde_json', 'version': '1.0.151', 'features': ['raw_value']}
print('Two mechanical identity forks: upstream Rust file sets and SHA-256 digests match.')

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

# S6d: core's complete implementation closure must not compile the bounds planner.
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
