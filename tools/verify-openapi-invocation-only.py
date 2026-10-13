"""Compile the minimal consumer against the preceding qualified archive snapshot."""
from pathlib import Path
import os,subprocess,json,hashlib,shutil
root=Path(__file__).resolve().parents[1]
target=Path(os.environ.get('CARGO_TARGET_DIR',root/'target'))
qualified=target/'openapi-json-path-qualification/archives'
receipt=qualified/'receipt.json'
if not receipt.is_file():raise SystemExit('Run archive qualification first')
data=json.loads(receipt.read_text())
for path,expected in data['testedInputs'].items():
    if hashlib.sha256((root/path).read_bytes()).hexdigest()!=expected:raise SystemExit('Qualified inputs changed')
out=qualified/'invocation-only';out.mkdir(exist_ok=True)
fixture=root/'examples/openapi-invocation-only'
if (out/'src').exists():shutil.rmtree(out/'src')
shutil.copytree(fixture/'src',out/'src')
# Preserve the already-qualified patch table verbatim; it contains only fixture plumbing.
text='\n[patch.crates-io]\n'+(qualified/'consumer/Cargo.toml').read_text().split('[patch.crates-io]\n',1)[1]
(out/'Cargo.toml').write_text((fixture/'Cargo.toml').read_text()+text)
shutil.copyfile(qualified/'consumer/Cargo.lock',out/'Cargo.lock')
for command in [['cargo','check'],['cargo','clippy','--locked','--','-D','warnings']]:subprocess.run(command,cwd=out,check=True)
tree=subprocess.check_output(['cargo','tree','--locked','--edges','normal'],cwd=out,text=True)
for forbidden in ['openbindings-json-schema-evaluator','reqwest','tokio','wasm-bindgen']:
    if forbidden in tree:raise SystemExit('Unexpected minimal consumer dependency: '+forbidden)
(out/'tree.txt').write_text(tree)
(out/'receipt.json').write_text(json.dumps({'archive_receipt_sha256':hashlib.sha256(receipt.read_bytes()).hexdigest(),'files':{str(p.relative_to(fixture)):hashlib.sha256(p.read_bytes()).hexdigest() for p in fixture.rglob('*') if p.is_file()},'status':'pass'},indent=2)+'\n')
print('Minimal invocation-only consumer passed')
