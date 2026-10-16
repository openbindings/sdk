#!/usr/bin/env python3
"""Serial adjacent-pair benchmarks with outcome and memory gates; timing is advisory."""
import argparse
import difflib
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import statistics as stats
import subprocess
import tempfile
import tomllib
import sys

from fixtures import fixtures


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


SDK_MODULE = 'src/sdk.rs'


def instrument_files(root):
    """Exclude actual build/cache artifacts, never similarly named source paths."""
    files = {}
    for path in root.rglob('*'):
        relative = path.relative_to(root)
        if relative.parts[0] in {'target', '__pycache__'}:
            continue
        if path.suffix == '.pyc' and '__pycache__' in relative.parts:
            continue
        if path.is_file() or path.is_symlink():
            files[relative.as_posix()] = path
    return files


def sdk_crates(*roots):
    return {tomllib.loads(p.read_text())['package']['name']
            for root in roots for p in (root/'crates').glob('*/Cargo.toml')}


def lock_without_sdk(data, names):
    """Permit edits only inside complete [[package]] blocks owned by the SDK.

    Preserve all other bytes, including third-party dependencies and metadata.
    Parse the whole lockfile too, so malformed or duplicate TOML cannot pass.
    """
    tomllib.loads(data.decode())
    # Split every table, so metadata or patch tables following an SDK package
    # cannot be discarded with that package's permitted lines.
    blocks = re.split(rb'(?m)(?=^[ \t]*\[)', data)
    return b''.join(block for block in blocks
                    if not block.startswith(b'[[package]]')
                    or tomllib.loads(block.decode())['package'][0]['name'] not in names)


def instrument_diff(baseline, candidate, names):
    before, after = instrument_files(baseline), instrument_files(candidate)
    changed, forbidden, diff = [], [], []
    for name in sorted(before.keys() | after.keys()):
        left, right = before.get(name), after.get(name)
        def contents(path):
            if path is None: return b''
            return os.fsencode(os.readlink(path)) if path.is_symlink() else path.read_bytes()
        a, b = contents(left), contents(right)
        mode_a = left.lstat().st_mode & 0o777 if left else None
        mode_b = right.lstat().st_mode & 0o777 if right else None
        link_a = left.is_symlink() if left else False
        link_b = right.is_symlink() if right else False
        if a == b and mode_a == mode_b and link_a == link_b and bool(left) == bool(right):
            continue
        changed.append(name)
        diff.append(f'file {name}: {mode_a} {hashlib.sha256(a).hexdigest()} -> {mode_b} {hashlib.sha256(b).hexdigest()}\n')
        diff.extend(difflib.unified_diff(a.decode(errors='backslashreplace').splitlines(True),
                                       b.decode(errors='backslashreplace').splitlines(True),
                                       fromfile='baseline/'+name, tofile='candidate/'+name))
        allowed = name == SDK_MODULE and left is not None and right is not None
        if name == 'Cargo.lock' and left and right and mode_a == mode_b:
            try:
                allowed = lock_without_sdk(a, names) == lock_without_sdk(b, names)
            except (ValueError, KeyError, UnicodeError):
                allowed = False
        if not allowed or (left and left.is_symlink()) or (right and right.is_symlink()):
            forbidden.append(name)
    content = ''.join(diff)
    return dict(confined=bool(changed) and not forbidden, changed=changed, forbidden=forbidden,
                diff=content, sha256=hashlib.sha256(content.encode()).hexdigest())


def build_instruments(baseline_source, candidate_source, output, binaries, target):
    """Try the identical candidate instrument first; audit before any fallback."""
    output.mkdir(parents=True, exist_ok=True)
    binaries.mkdir(parents=True, exist_ok=True)
    base = baseline_source/'tools/evaluator-bench'
    candidate = candidate_source/'tools/evaluator-bench'
    audit = instrument_diff(base, candidate, sdk_crates(baseline_source, candidate_source))
    (output/'instrument.diff').write_text(audit.pop('diff'))
    metadata = dict(kind='identical', instrument_diff=audit,
                    baseline_source=str(baseline_source.resolve()), candidate_source=str(candidate_source.resolve()))
    metadata['lock_sha256'] = dict(baseline=digest(base/'Cargo.lock'), candidate=digest(candidate/'Cargo.lock'))
    # Never overwrite the original merge-base instrument. Its own relative SDK
    # paths still resolve correctly from this temporary sibling directory.
    with tempfile.TemporaryDirectory(prefix='evaluator-candidate-', dir=base.parent) as temporary:
        same = Path(temporary)
        for name, source in instrument_files(candidate).items():
            destination = same/name
            destination.parent.mkdir(parents=True, exist_ok=True)
            if source.is_symlink():
                destination.symlink_to(os.readlink(source))
            else:
                shutil.copy2(source, destination)
        def build(source, variant, allocation=False, attempt=''):
            command = ['cargo', 'build', '--locked', '--release', '--manifest-path', str(source/'Cargo.toml')]
            if allocation: command += ['--features', 'allocations']
            suffix = '-alloc' if allocation else ''
            env = dict(os.environ, CARGO_TARGET_DIR=str(target.resolve()))
            with (output/f'build-{variant}{suffix}{attempt}.log').open('w') as log:
                code = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, env=env).returncode
            if code == 0:
                shutil.copy2(target/'release/openbindings-evaluator-bench', binaries/(variant+suffix))
            return code == 0
        # Candidate failures are real build failures, never "not comparable".
        if not build(candidate, 'candidate') or not build(candidate, 'candidate', True):
            raise RuntimeError('Candidate release instrument failed; inspect build logs')
        baseline_builds = build(same, 'baseline', attempt='-identical')
        if baseline_builds:
            baseline_builds = build(same, 'baseline', True, attempt='-identical')
        if not baseline_builds:
            if not audit['confined']:
                metadata['kind'] = 'not comparable'
                metadata['reason'] = 'Candidate instrument failed against merge base; instrument diff is not confined to src/sdk.rs and SDK-owned lock entries.'
            else:
                metadata['kind'] = 'cross-API'
                if not build(base, 'baseline') or not build(base, 'baseline', True):
                    raise RuntimeError('Confined cross-API baseline failed; inspect build logs')
    (output/'instrument-pair.json').write_text(json.dumps(metadata, indent=2)+'\n')
    if metadata['kind'] == 'not comparable':
        (output/'summary.md').write_text('# Evaluator performance\n\nNot comparable: '+metadata['reason']+'\n')
    return metadata


def build_main():
    parser = argparse.ArgumentParser(description='Build identical or strictly confined cross-API release pairs')
    for key in ['baseline-source', 'candidate-source', 'output', 'binaries', 'target']:
        parser.add_argument('--'+key, type=Path, required=True)
    args = parser.parse_args(sys.argv[2:])
    metadata = build_instruments(args.baseline_source, args.candidate_source, args.output, args.binaries, args.target)
    print(metadata['kind'])
    github_output = os.environ.get('GITHUB_OUTPUT')
    if github_output:
        with open(github_output, 'a') as stream:
            stream.write('comparable='+str(metadata['kind'] != 'not comparable').lower()+'\n')


def interval(values, rng):
    bootstrap = sorted(stats.median(rng.choices(values, k=len(values))) for _ in range(5000))
    return [bootstrap[125], bootstrap[4875]]


def summary(pairs, rng):
    base, candidate = zip(*pairs)
    ratios = [c / b if b else (1.0 if not c else None) for b, c in pairs]
    finite = all(r is not None for r in ratios)
    bm, cm = stats.median(base), stats.median(candidate)
    return dict(base=bm, candidate=cm, ratio=stats.median(ratios) if finite else None,
                ci95=interval(ratios, rng) if finite else None,
                difference=stats.median([c-b for b,c in pairs]),
                difference_ci95=interval([c-b for b,c in pairs], rng),
                base_mad_fraction=stats.median(abs(x-bm) for x in base)/bm if bm else 0,
                candidate_mad_fraction=stats.median(abs(x-cm) for x in candidate)/cm if cm else 0)


def time_decision(row, threshold, floor):
    if max(row['base_mad_fraction'], row['candidate_mad_fraction']) > .10:
        return 'inconclusive'
    low, high = row['ci95']
    if row['ratio'] > threshold and row['difference'] > floor and low > threshold:
        return 'regression'
    if low <= threshold < high and row['difference'] > floor:
        return 'inconclusive'
    return 'within-threshold'


def confirm(decisions):
    if len(decisions) < 2 or len(set(decisions)) != 1 or 'inconclusive' in decisions:
        return 'inconclusive'
    return decisions[0]


def memory_regression(row, relative, absolute):
    return row['candidate'] > row['base'] * relative and row['difference'] > absolute


def enforcement_fails(report):
    return bool(report['errors']) or any(
        row.get('heap_regression') or row.get('allocation_regression')
        for row in report['rows']
    )


def invoke(binary, request, log, rss=False):
    command = [str(binary.resolve())]
    if request["mode"] == "empty-process": command.append("--empty")
    if rss and hasattr(os, 'wait4'):
        # Files prevent pipe backpressure; wait4 reports this child, not the
        # high-water mark of all previously launched children.
        with tempfile.TemporaryFile() as stdin, tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            stdin.write(json.dumps(request).encode()); stdin.seek(0)
            child = subprocess.Popen(command, stdin=stdin, stdout=stdout, stderr=stderr)
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            stdout.seek(0); stderr.seek(0)
            output, error = stdout.read().decode(), stderr.read().decode()
            code = child.returncode
            rss_bytes = int(usage.ru_maxrss * (1 if platform.system() == 'Darwin' else 1024))
    else:
        run = subprocess.run(command, input=json.dumps(request), text=True, capture_output=True, timeout=120)
        output, error, code = run.stdout, run.stderr, run.returncode
        rss_bytes = None
    log.write(json.dumps(dict(command=command, mode=request['mode'], iterations=request['iterations'],
                              returncode=code, stdout=output, stderr=error, rss_bytes=rss_bytes))+'\n'); log.flush()
    if code:
        raise RuntimeError(f'{binary.name} exited {code}: {error}')
    answer = json.loads(output)
    assert answer['iterations'] == request['iterations']
    assert sum(answer['outcomes'].values()) == request['iterations']
    if rss_bytes is not None:
        answer['rss_bytes'] = rss_bytes
    return answer


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ['baseline', 'candidate', 'baseline-alloc', 'candidate-alloc', 'output']:
        parser.add_argument('--'+key, type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=20)
    parser.add_argument('--batches', type=int, default=2)
    parser.add_argument('--seconds', type=float, default=.2)
    parser.add_argument('--workloads', help='Comma-separated subset for one-command replay')
    parser.add_argument('--dedicated', action='store_true')
    parser.add_argument('--enforce', action='store_true', help='Fail on outcome errors and heap/count flags; timing and RSS remain advisory')
    args = parser.parse_args()
    assert args.pairs >= 2 and args.batches >= 1 and args.seconds > 0
    if args.enforce:
        assert args.pairs >= 20 and args.batches >= 2 and args.seconds >= .2, 'Enforcement requires the full sampling policy'
    args.output.mkdir(parents=True, exist_ok=True)
    result_path = args.output/'comparison.json'
    if result_path.exists():
        raise SystemExit('Refusing to overwrite an existing comparison')
    pair_path = args.output/'instrument-pair.json'
    instrument_pair = json.loads(pair_path.read_text()) if pair_path.exists() else dict(kind='identical')
    if instrument_pair['kind'] == 'not comparable':
        raise SystemExit('Refusing to measure a pair marked not comparable')
    selected = fixtures()
    if args.workloads:
        selected = {name:selected[name] for name in args.workloads.split(',')}
    rng = random.Random(20261015)
    report = dict(instrument_pair=instrument_pair, advisory=not args.enforce, timing_advisory=True, host=dict(platform=platform.platform(), machine=platform.machine(), processor=platform.processor()),
                  policy=dict(pairs=args.pairs, batches=args.batches, seconds=args.seconds, time_ratio=1.10 if args.dedicated else 1.20,
                              heap_ratio=1.10, heap_bytes=128*1024, allocation_ratio=1.05, allocation_count=32, rss_ratio=1.15, rss_bytes=1024*1024),
                  binaries={key:dict(path=str(getattr(args,key).resolve()), sha256=digest(getattr(args,key))) for key in ['baseline','candidate','baseline_alloc','candidate_alloc']},
                  rows=[], errors=[])
    (args.output/'fixtures.json').write_text(json.dumps(selected, indent=2)+'\n')
    report['fixtures_sha256'] = digest(args.output/'fixtures.json')
    for command in [['rustc','-Vv'], ['cargo','-V']]:
        run = subprocess.run(command, capture_output=True, text=True)
        report[command[0]] = run.stdout
    with (args.output/'commands.jsonl').open('w') as log:
        report['empty_process'] = {key:[invoke(getattr(args,key), dict(mode='empty-process',iterations=1), log, rss=True) for _ in range(3)]
                                   for key in ['baseline','candidate','baseline_alloc','candidate_alloc']}
        for name, fixture in selected.items():
            request = {key:value for key,value in fixture.items() if key != 'modes'}
            for mode in fixture['modes']:
                row = dict(workload=name, mode=mode, samples=[], memory_samples=[], batches=[])
                report['rows'].append(row)
                try:
                    def run(variant, n, allocation=False, rss=False):
                        binary = getattr(args,variant+('_alloc' if allocation else ''))
                        return invoke(binary, dict(request, mode=mode, iterations=n), log, rss)
                    # Complete observations (including refusal codes and source
                    # diagnostics) must agree before either timing is considered.
                    initial = {v:run(v, 1) for v in ['baseline','candidate']}
                    assert initial['baseline']['observation'] == initial['candidate']['observation'], initial
                    row['observation'] = initial['baseline']['observation']
                    counts = {}
                    for v in ['baseline','candidate']:
                        n = 1
                        for _ in range(3): run(v, n)
                        if mode != 'cold':
                            for _ in range(16):
                                sample = run(v, n)
                                if sample['nanoseconds'] >= args.seconds*1e9: break
                                n = max(n+1, math.ceil(n*args.seconds*1.1e9/max(1,sample['nanoseconds'])))
                            else: raise RuntimeError('Batch calibration did not converge')
                        counts[v] = n
                    for batch in range(args.batches):
                        pairs = []
                        for index in range(args.pairs):
                            order = ['baseline','candidate']; rng.shuffle(order)
                            pair = {v:run(v,counts[v]) for v in order}
                            for answer in pair.values():
                                assert answer['observation'] == row['observation'], answer
                            row['samples'].append(dict(batch=batch, pair=index, order=order, **pair))
                            pairs.append(tuple(pair[v]['nanoseconds']/counts[v] for v in ['baseline','candidate']))
                        measured = summary(pairs,rng)
                        floor = 2000 if mode in ['cold','compile','context','document'] else 10000 if name in ['document-1000','array','large-object','refs','large-literal','unique-array','late-invalid','late-duplicate','decimal-array','regex-failure','regex-stop'] else 20
                        measured['decision'] = time_decision(measured,report['policy']['time_ratio'],floor)
                        row['batches'].append(measured)
                    row['time_decision'] = confirm([b['decision'] for b in row['batches']])
                    for _ in range(3):
                        order = ['baseline','candidate']; rng.shuffle(order)
                        pair = {v:run(v,1,allocation=True) for v in order}
                        for answer in pair.values(): assert answer['observation'] == row['observation']
                        row['memory_samples'].append(pair)
                    row['memory'] = {metric:summary([(s['baseline']['memory'][metric],s['candidate']['memory'][metric]) for s in row['memory_samples']],rng)
                                     for metric in ['baseline_live_bytes','peak_live_bytes','incremental_peak_bytes','allocations_per_op','requested_bytes_per_op']}
                    # Gate incremental operation memory; keep the retained owner
                    # and total peak visible so baseline increases cannot hide.
                    row['heap_regression'] = any(memory_regression(row['memory'][key],1.10,128*1024) for key in ['baseline_live_bytes','peak_live_bytes','incremental_peak_bytes','requested_bytes_per_op'])
                    row['allocation_regression'] = memory_regression(row['memory']['allocations_per_op'],1.05,32)
                    row['rss_samples'] = []
                    for _ in range(3):
                        order = ['baseline','candidate']; rng.shuffle(order)
                        pair = {v:run(v,1,rss=True) for v in order}
                        for answer in pair.values(): assert answer['observation'] == row['observation']
                        row['rss_samples'].append(pair)
                    if 'rss_bytes' in row['rss_samples'][0]['baseline']:
                        row['rss'] = summary([(s['baseline']['rss_bytes'],s['candidate']['rss_bytes']) for s in row['rss_samples']],rng)
                        row['rss_needs_verification'] = memory_regression(row['rss'],1.15,1024*1024)
                    print(name,mode,row['time_decision'],flush=True)
                except Exception as error:
                    row['error'] = str(error)
                    report['errors'].append(dict(workload=name,mode=mode,error=str(error)))
                    print(name,mode,'ERROR',error,flush=True)
                result_path.write_text(json.dumps(report,indent=2)+'\n')
    failures = enforcement_fails(report)
    (args.output/'summary.md').write_text('# Evaluator performance\n\nPair: '+instrument_pair['kind']+'.\n\n'+('Advisory run: outcome errors and heap/count flags are reported without failing.\n\n' if not args.enforce else 'Outcome checks and requested heap/allocation-count thresholds are enforced. Timing and RSS remain advisory.\n\n')+
        '| Workload | Mode | Candidate/base | Time decision | Heap/count flag |\n| --- | --- | ---: | --- | --- |\n'+''.join(
            f"| {r['workload']} | {r['mode']} | {r['batches'][-1]['ratio']:.3f} | {r.get('time_decision','error')} | {bool(r.get('heap_regression') or r.get('allocation_regression'))} |\n" if r['batches'] else f"| {r['workload']} | {r['mode']} | n/a | error | n/a |\n" for r in report['rows']))
    if args.enforce and failures: raise SystemExit(1)

if __name__ == '__main__':
    if sys.argv[1:2] == ['build']:
        build_main()
    else:
        main()
