"""Authenticated source scaling and negative/anonymous-path diagnostics, not gates.
Requires a fresh extracted-package qualification receipt. Retains all three runs.
"""
from pathlib import Path
import argparse, hashlib, json, os, platform, statistics, subprocess

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
target = Path(os.environ.get('CARGO_TARGET_DIR', root / 'target'))
consumer = target / 'openapi-json-path-qualification/archives/consumer'
receipt = consumer.parent / 'receipt.json'
for relative, expected in json.loads(receipt.read_text())['testedInputs'].items():
    if hashlib.sha256((root / relative).read_bytes()).hexdigest() != expected:
        raise SystemExit('Stale qualification: ' + relative)
build = subprocess.run(['cargo', 'build', '--release', '--locked', '--manifest-path', str(consumer / 'Cargo.toml'), '--example', 'auth_diagnostics'], text=True, capture_output=True)
(args.output / 'build.log').write_text(build.stdout + build.stderr)
if build.returncode:
    raise SystemExit(build.returncode)
binary = target / 'release/examples/auth_diagnostics'
raw = []
for process in range(3):
    print('diagnostic process', process + 1, flush=True)
    result = subprocess.run([str(binary)], text=True, capture_output=True)
    (args.output / f'{process}.stdout').write_text(result.stdout)
    (args.output / f'{process}.stderr').write_text(result.stderr)
    if result.returncode:
        raise SystemExit(result.returncode)
    raw.append(json.loads(result.stdout))
(args.output / 'raw.json').write_text(json.dumps(raw, indent=2) + '\n')
rows = []
for index, example in enumerate(raw[0]['rows']):
    medians = [statistics.median(process['rows'][index]['samples_ns']) for process in raw]
    rows.append({**{k: v for k, v in example.items() if k != 'samples_ns'}, 'process_medians_ns': medians, 'median_ns': statistics.median(medians), 'process_spread': (max(medians)-min(medians))/statistics.median(medians)})
summary = {'format': 'openbindings.auth-json.diagnostics@1', 'rows': rows, 'notes': raw[0]['notes'], 'host': platform.platform(), 'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(), 'receipt_sha256': hashlib.sha256(receipt.read_bytes()).hexdigest(), 'source_sha256': hashlib.sha256((consumer/'examples/auth_diagnostics.rs').read_bytes()).hexdigest(), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
(args.output/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
print('diagnostics complete; no equivalent direct coverage denominator asserted')
