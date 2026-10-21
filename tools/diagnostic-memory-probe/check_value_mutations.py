"""Prove allocator regressions reject eager default-value errors in an isolated copy."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
out = root / 'target/diagnostics-review/value-mutations'
out.mkdir(parents=True, exist_ok=True)
copy = out / 'sdk'
copy.mkdir(exist_ok=True)
ignore = shutil.ignore_patterns('.git', 'target', 'node_modules', '__pycache__')
for name in ['Cargo.toml', 'Cargo.lock']:
    shutil.copy2(root / name, copy / name)
for name in ['crates', 'tools/diagnostic-memory-probe']:
    shutil.copytree(root / name, copy / name, dirs_exist_ok=True, ignore=ignore)
source = Path('crates/openbindings-json-schema-evaluator/src/lib.rs')
original = (root / source).read_bytes()
text = original.decode()
env = os.environ.copy()
env['CARGO_NET_OFFLINE'] = 'true'
env['CARGO_TARGET_DIR'] = str(out / 'target')
records = []
try:
    # The cheap check has one validity call; each failing path must catch its mutation.
    needle = 'self.validator.is_valid(backend::view(value))'
    replacement = 'self.validator.validate(backend::view(value)).is_ok()'
    assert text.count(needle) == 1, text.count(needle)
    for path in ['ordinary', 'referenced']:
        # The probe stops at the first rejected sample, so each path runs alone.
        (copy / source).write_text(text.replace(needle, replacement), encoding='utf-8', newline='\n')
        command = ['cargo', 'run', '--offline', '--release', '--locked', '--manifest-path',
                   str(copy / 'tools/diagnostic-memory-probe/Cargo.toml'), '--', '--values-only', '--path', path]
        run = subprocess.run(command, cwd=copy, env=env, capture_output=True, text=True)
        (out / (path + '.jsonl')).write_text(run.stdout, encoding='utf-8', newline='\n')
        (out / (path + '.log')).write_text(run.stderr, encoding='utf-8', newline='\n')
        rows = [json.loads(line) for line in run.stdout.splitlines()]
        rejected = [r for r in rows if r.get('path') == path and r['peak_bytes'] > r['ceiling_bytes']]
        assert run.returncode != 0 and rejected and 'coarse allocation:' in run.stderr, (path, run.returncode, run.stderr[-1200:])
        records.append({'path': path, 'mutation': replacement, 'exit_code': run.returncode,
                        'rejected_samples': rejected, 'command': command})
finally:
    (copy / source).write_bytes(original)
    assert (root / source).read_bytes() == original, 'production evaluator changed'
receipt = {'source_sha256': hashlib.sha256(original).hexdigest(), 'status': 'all mutations rejected by allocation', 'mutations': records}
(out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps({'status': receipt['status'], 'peaks': {r['path']: r['rejected_samples'][0]['peak_bytes'] for r in records}}))
