"""One preregistered sampler-resolution experiment. Does not grade the product.

A completed archive qualification is required. The output path must be new.
All subprocess output, including failures, is retained. No retries are performed.
"""
from pathlib import Path
import argparse
import gzip
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
MODES = ['direct-anonymous', 'direct-auth', 'adapter-anonymous', 'adapter-auth']
WILLIAMS = [[0, 1, 3, 2], [1, 2, 0, 3], [2, 3, 1, 0], [3, 0, 2, 1]]
CONFIGS = [('legacy-50ms', False), ('legacy-50ms', True), ('balanced-200ms', False), ('balanced-200ms', True)]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def environment():
    def read(command):
        result = subprocess.run(command, capture_output=True, text=True)
        return {'exit': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}
    return {'time_ns': time.time_ns(), 'load_average': os.getloadavg(),
            'cpu': read(['sysctl', '-n', 'machdep.cpu.brand_string']),
            'performance_levels': read(['sysctl', 'hw.nperflevels', 'hw.perflevel0.physicalcpu', 'hw.perflevel1.physicalcpu']),
            'thermal': read(['pmset', '-g', 'therm'])}


def validate_and_summarize(raw, process):
    assert raw['investigation'] and raw['process'] == process and not raw['anonymous_only']
    assert len(raw['rows']) == 16
    result = []
    for row_index in range(4):
        expected_configs = CONFIGS[(row_index + process) % 4:] + CONFIGS[:(row_index + process) % 4]
        for config_index, (sampler, null) in enumerate(expected_configs):
            row = raw['rows'][row_index * 4 + config_index]
            assert row['configuration_index'] == config_index
            assert (row['sampler'], row['null_control']) == (sampler, null)
            assert row['version'] == ['3.0.4', '3.1.2'][row_index // 2]
            assert row['layer'] == ['binding', 'operation'][row_index % 2]
            assert row['workload'] == 'and-post' and row['bytes'] == 65536
            assert row['context_bytes'] == (0 if null else [334, 332][row_index // 2])
            assert set(row['samples_ns']) == set(MODES)
            for sample in range(7):
                blocks = [b for b in row['blocks'] if b['sample'] == sample]
                if sampler == 'balanced-200ms':
                    assert len(blocks) % 16 == 0
                    for i, b in enumerate(blocks):
                        cycle, within = divmod(i, 16)
                        order, position = divmod(within, 4)
                        assert (b['cycle'], b['order'], b['position']) == (cycle, order, position)
                        assert b['mode'] == MODES[WILLIAMS[order][position]]
                else:
                    assert len(blocks) == 4
                    assert [b['mode'] for b in blocks] == (MODES[::-1] if sample % 2 else MODES)
                for mode in MODES:
                    matched = [b for b in blocks if b['mode'] == mode]
                    assert all(b['calls'] > 0 and b['elapsed_ns'] >= (2_000_000 if sampler == 'balanced-200ms' else 50_000_000) for b in matched)
                    ns, calls = sum(b['elapsed_ns'] for b in matched), sum(b['calls'] for b in matched)
                    assert ns >= (200_000_000 if sampler == 'balanced-200ms' else 50_000_000)
                    samples = row['samples_ns'][mode]
                    assert len(samples) == 7 and abs(samples[sample] - ns / calls) < 1e-6
            medians = {mode: statistics.median(samples) for mode, samples in row['samples_ns'].items()}
            direct = medians['direct-auth'] - medians['direct-anonymous']
            candidate = medians['adapter-auth'] - medians['adapter-anonymous']
            residual = candidate - direct
            ceiling = max(0, direct) + 5000 + 2 * row['context_bytes']
            result.append({k: row[k] for k in ['version', 'workload', 'layer', 'sampler', 'null_control', 'context_bytes', 'samples_ns']} |
                          {'process': process, 'medians_ns': medians, 'direct_delta_ns': direct,
                           'candidate_delta_ns': candidate, 'four_median_residual_ns': residual,
                           'frozen_formula_ceiling_ns': ceiling,
                           'formula_slack_ns': ceiling - candidate,
                           'diagnostic_formula_result': 'pass' if candidate <= ceiling else 'fail',
                           'null_resolution_pass': all(abs(x) <= 1000 for x in [direct, candidate, residual]) if null else None})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    consumer = target / 'openapi-json-path-qualification/archives/consumer'
    receipt = consumer.parent / 'receipt.json'
    qualified = json.loads(receipt.read_text())
    for rel, expected in qualified['testedInputs'].items():
        if sha(ROOT / rel) != expected:
            raise SystemExit('Stale qualification ' + rel)
    measurement = ROOT / 'examples/openapi-json-path-consumer/examples/auth_measure.rs'
    assert sha(measurement) == sha(consumer / 'examples/auth_measure.rs')
    command = ['cargo', 'build', '--release', '--manifest-path', str(consumer / 'Cargo.toml'), '--example', 'auth_measure']
    build = subprocess.run(command, text=True, capture_output=True)
    (args.output / 'build.log').write_text(build.stdout + build.stderr)
    if build.returncode:
        raise SystemExit('Build failed; no measurements run')
    binary = args.output / 'candidate'
    shutil.copyfile(target / 'release/examples/auth_measure', binary)
    binary.chmod(0o755)
    meta = {'format': 'openbindings.auth-performance-investigation@1',
            'host': platform.platform(), 'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
            'measurement_sha256': sha(measurement), 'runner_sha256': sha(Path(__file__)),
            'preregistration_sha256': sha(ROOT / 'docs/evidence/openapi-auth-json/performance-investigation-plan.md'),
            'receipt_sha256': sha(receipt), 'binary_sha256': sha(binary),
            'qualification_runner_sha256': sha(ROOT / 'tools/measure-openapi-auth-json.py'),
            'settle_seconds': 10, 'processes': [], 'status': 'running'}
    (args.output / 'provenance.json').write_text(json.dumps(meta, indent=2) + '\n')
    # Fixed once, never wait conditionally for a favorable load reading.
    time.sleep(10)
    rows = []
    for process in range(3):
        print(f'diagnostic process {process + 1}/3', flush=True)
        record = {'process': process, 'before': environment()}
        run = subprocess.run([str(binary), 'investigation', f'process={process}'], text=True, capture_output=True)
        record.update(after=environment(), exit=run.returncode)
        with gzip.open(args.output / f'process-{process}.stdout.json.gz', 'wb') as compressed:
            compressed.write(run.stdout.encode())
        (args.output / f'process-{process}.stderr').write_text(run.stderr)
        meta['processes'].append(record)
        (args.output / 'provenance.json').write_text(json.dumps(meta, indent=2) + '\n')
        if run.returncode:
            raise SystemExit('Diagnostic process failed; no retry')
        rows += validate_and_summarize(json.loads(run.stdout), process)
    balanced_null = [r for r in rows if r['null_control'] and r['sampler'] == 'balanced-200ms']
    adequate = all(r['null_resolution_pass'] for r in balanced_null)
    comparison = []
    for version in ['3.0.4', '3.1.2']:
        for layer in ['binding', 'operation']:
            for sampler in ['legacy-50ms', 'balanced-200ms']:
                group = [r for r in rows if r['null_control'] and r['version'] == version and r['layer'] == layer and r['sampler'] == sampler]
                comparison.append({'version': version, 'layer': layer, 'sampler': sampler,
                                   'null_direct_delta_sd_ns': statistics.stdev(r['direct_delta_ns'] for r in group),
                                   'null_candidate_delta_sd_ns': statistics.stdev(r['candidate_delta_ns'] for r in group),
                                   'null_residual_sd_ns': statistics.stdev(r['four_median_residual_ns'] for r in group),
                                   'max_absolute_null_quantity_ns': max(abs(r[k]) for r in group for k in ['direct_delta_ns', 'candidate_delta_ns', 'four_median_residual_ns'])})
    summary = {'format': 'openbindings.auth-performance-investigation@1',
               'status': 'method-adequate-for-one-qualification' if adequate else 'inconclusive-stop',
               'product_qualified_complete': False, 'null_resolution_limit_ns': 1000,
               'balanced_null_passes': sum(r['null_resolution_pass'] for r in balanced_null),
               'balanced_null_total': len(balanced_null), 'rows': rows, 'comparison': comparison,
               'notes': ['Diagnostic formula results do not qualify or regrade the product.',
                         'No null correction; differences of condition medians retained.',
                         'A resolution guard failure stops further qualification in this investigation.']}
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    meta['status'] = summary['status']
    (args.output / 'provenance.json').write_text(json.dumps(meta, indent=2) + '\n')
    print(json.dumps({k: summary[k] for k in ['status', 'balanced_null_passes', 'balanced_null_total']}), flush=True)
    return 0 if adequate else 2


if __name__ == '__main__':
    raise SystemExit(main())
