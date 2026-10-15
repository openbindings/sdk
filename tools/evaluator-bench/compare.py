#!/usr/bin/env python3
"""Serial adjacent-pair benchmarks. Advisory until explicitly promoted."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import statistics as stats
import subprocess
import tempfile

from fixtures import fixtures


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


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
    parser.add_argument('--enforce', action='store_true', help='Promote only after runner calibration')
    args = parser.parse_args()
    assert args.pairs >= 2 and args.batches >= 1 and args.seconds > 0
    if args.enforce:
        assert args.pairs >= 20 and args.batches >= 2 and args.seconds >= .2, 'Enforcement requires the full sampling policy'
    args.output.mkdir(parents=True, exist_ok=True)
    result_path = args.output/'comparison.json'
    if result_path.exists():
        raise SystemExit('Refusing to overwrite an existing comparison')
    selected = fixtures()
    if args.workloads:
        selected = {name:selected[name] for name in args.workloads.split(',')}
    rng = random.Random(20261015)
    report = dict(advisory=not args.enforce, host=dict(platform=platform.platform(), machine=platform.machine(), processor=platform.processor()),
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
                        floor = 2000 if mode in ['cold','compile','context'] else 10000 if name in ['array','large-object','refs','large-literal','unique-array','late-invalid','late-duplicate','decimal-array','regex-failure','regex-stop'] else 20
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
                    row['heap_regression'] = any(memory_regression(row['memory'][key],1.10,128*1024) for key in ['baseline_live_bytes','peak_live_bytes','incremental_peak_bytes'])
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
    failures = report['errors'] or any(r.get('time_decision') != 'within-threshold' or r.get('heap_regression') or r.get('allocation_regression') for r in report['rows'])
    (args.output/'summary.md').write_text('# Evaluator performance\n\n'+('Advisory: no build failures during calibration.\n\n' if not args.enforce else 'Enforced paired thresholds.\n\n')+
        '| Workload | Mode | Candidate/base | Time decision | Heap/count flag |\n| --- | --- | ---: | --- | --- |\n'+''.join(
            f"| {r['workload']} | {r['mode']} | {r['batches'][-1]['ratio']:.3f} | {r.get('time_decision','error')} | {bool(r.get('heap_regression') or r.get('allocation_regression'))} |\n" if r['batches'] else f"| {r['workload']} | {r['mode']} | — | error | — |\n" for r in report['rows']))
    if args.enforce and failures: raise SystemExit(1)

if __name__ == '__main__':
    main()
