"""Keep the two package-identity forks mechanical, and their private graph wired."""
from pathlib import Path
import argparse
import hashlib
import json
import tarfile
import tomllib

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--upstream-dir', type=Path, help='Also verify the original downloaded crate archives')
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
records = json.loads((root / 'docs/dependency-isolation.json').read_text())['mechanical_packages']
for record in records:
    package = root / 'vendor' / record['package']
    actual = {str(path.relative_to(package)): hashlib.sha256(path.read_bytes()).hexdigest()
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
