"""Replay independent container/child failures through native and TypeScript APIs."""
from pathlib import Path
import argparse
import base64
import json
import subprocess

root = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
args = parser.parse_args()
cases = json.loads((root / 'tools/qualification/inputs/container-findings-cases.json').read_text(encoding='utf-8'))
out = root / 'target/diagnostics-review'
out.mkdir(parents=True, exist_ok=True)
requests = ''.join(json.dumps({'id':case['id'], 'action':'validate-document', 'given':{
    'documentBase64':base64.b64encode(case['document'].encode()).decode()}})+'\n' for case in cases)
request_file = out / 'container-requests.jsonl'
request_file.write_text(requests, encoding='utf-8', newline='\n')
native = subprocess.run([str(args.binary)], input=requests, capture_output=True, text=True, check=True)
typescript = subprocess.run(['node',str(root / 'tools/qualification/replay/node-corpus.mjs'),str(request_file)], capture_output=True, text=True, check=True)

def canonical(value):
    if isinstance(value, list): return [canonical(x) for x in value]
    if isinstance(value, dict):
        return {{'byteOffset':'byte_offset','byteColumn':'byte_column'}.get(k,k):canonical(v) for k,v in value.items()}
    return value

rows = []
for case, a, b in zip(cases, native.stdout.splitlines(), typescript.stdout.splitlines(), strict=True):
    a, b = json.loads(a), canonical(json.loads(b))
    assert a == b, case['id']
    fixed = [f for f in a['findings'] if f['rule'] == 'OBI-02']
    assert [f['location']['pointer'] for f in fixed] == case['obi02_pointers'], case['id']
    assert all(f['message'] == 'value violates the fixed normative schema' for f in fixed)
    assert a['outcome'] == 'non-conformant' and a['rules']['OBI-02'] == 'violated'
    rows.append({'id':case['id'],'native_and_typescript':a})
(out / 'container-findings-parity.json').write_text(json.dumps(rows,indent=2)+'\n', encoding='utf-8', newline='\n')
print(json.dumps({'cases':len(rows),'status':'exact native/TypeScript parity; all child and independent container locations retained'}))
