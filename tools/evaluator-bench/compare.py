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
FIXTURES = 'fixtures.py'
RESIDUALS = 'accepted-residuals.json'
ACCEPTED_RESIDUALS = Path(__file__).with_name(RESIDUALS)
RESIDUAL_FORMAT = 'openbindings.evaluator-bench.accepted-residuals@1'
HEAP_METRICS = ['baseline_live_bytes', 'peak_live_bytes', 'incremental_peak_bytes', 'requested_bytes_per_op']
ALLOCATION_METRIC = 'allocations_per_op'
RESIDUAL_METRICS = HEAP_METRICS + [ALLOCATION_METRIC, 'time']


def accepted_residuals(path):
    """The regressions a change declares accepted, checked before anything is measured.

    Each entry names one exact workload and mode, one metric (a requested-heap
    metric, `allocations_per_op` or `time`), the largest candidate/base ratio
    accepted, and the reason, which names the decision that accepted it. No
    file means no accepted residual.
    """
    if not path.exists():
        return []
    def refuse(problem):
        raise ValueError(f'{path}: {problem}')
    data = json.loads(path.read_text(encoding='utf-8'))
    if not isinstance(data, dict) or set(data) != {'format', 'residuals'}:
        refuse('expected exactly the members "format" and "residuals"')
    if data['format'] != RESIDUAL_FORMAT:
        refuse(f'format must be {RESIDUAL_FORMAT!r}')
    if not isinstance(data['residuals'], list):
        refuse('"residuals" must be an array')
    seen = set()
    for index, entry in enumerate(data['residuals']):
        fields = {'workload', 'mode', 'metric', 'max_ratio', 'reason'}
        if not isinstance(entry, dict) or set(entry) != fields:
            refuse(f'residual {index} must have exactly the members {sorted(fields)}')
        for field in ['workload', 'mode', 'metric', 'reason']:
            if not isinstance(entry[field], str) or not entry[field].strip():
                refuse(f'residual {index} needs a non-empty {field}')
        if entry['metric'] not in RESIDUAL_METRICS:
            refuse(f'residual {index} names unknown metric {entry["metric"]!r}')
        bound = entry['max_ratio']
        if isinstance(bound, bool) or not isinstance(bound, (int, float)) or not math.isfinite(bound) or bound <= 1:
            refuse(f'residual {index} needs a finite max_ratio above 1')
        key = (entry['workload'], entry['mode'], entry['metric'])
        if key in seen:
            refuse(f'residual {index} repeats {key}')
        seen.add(key)
    return data['residuals']


def residual_ratio(row, metric):
    """The candidate/base ratio a residual bounds: a memory metric's median
    paired ratio, or the largest batch ratio for time."""
    if metric == 'time':
        return max(batch['ratio'] for batch in row['batches'])
    return row['memory'][metric]['ratio']


def apply_residuals(report, residuals, selected, candidate_fixtures):
    """Judge every declared residual against the measured rows.

    A declared metric the policy flags is accepted within its bound and exceeded
    beyond it. An entry whose row does not exist, was not compared, or whose
    metric the policy does not flag is stale. Memory flags not accepted, and
    every undeclared flag, still fail enforcement; time remains advisory.
    """
    rows = {(row['workload'], row['mode']): row for row in report['rows']}
    judged = []
    for entry in residuals:
        result = dict(entry, ratio=None)
        workload, mode, metric = entry['workload'], entry['mode'], entry['metric']
        row = rows.get((workload, mode))
        if row is None:
            known = mode in candidate_fixtures.get(workload, {}).get('modes', [])
            result['status'] = 'not measured' if known and workload not in selected else 'stale'
            result['detail'] = 'not selected for this run' if result['status'] == 'not measured' else 'no such workload and mode'
        elif 'error' in row:
            result['status'], result['detail'] = 'error', 'the row did not complete'
        elif row['comparison'] != COMPARED:
            result['status'], result['detail'] = 'stale', 'the row is not compared: ' + row['comparison']
        else:
            flagged = row['time_decision'] == 'regression' if metric == 'time' else metric in row['memory_flags']
            result['ratio'] = residual_ratio(row, metric)
            if not flagged:
                result['status'], result['detail'] = 'stale', 'the policy no longer flags this metric'
            elif result['ratio'] <= entry['max_ratio']:
                result['status'], result['detail'] = 'accepted', 'within the declared bound'
                row.setdefault('accepted_residuals', []).append(metric)
            else:
                result['status'], result['detail'] = 'exceeded', 'beyond the declared bound'
        judged.append(result)
    return judged


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
    # The SDK's own crates and its renamed dependency forks, whose lock entries it owns.
    return {tomllib.loads(p.read_text(encoding='utf-8'))['package']['name']
            for root in roots for owned in ('crates', 'vendor')
            for p in (root/owned).glob('*/Cargo.toml')}


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


def requests(rows):
    """Each workload's request: its fixture row without the expectation."""
    return {name: {key: value for key, value in row.items() if key != 'expected'}
            for name, row in json.loads(json.dumps(rows)).items()}


def instrument_diff(baseline, candidate, names, same_requests=False):
    """Audit the instrument diff. `same_requests` states that both fixture
    generators produce the same requests, so a fixture edit can only change
    expectations, which declare outcome changes the measuring step checks per side.
    """
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
        if name == FIXTURES and left and right and mode_a == mode_b:
            allowed = same_requests
        # Declared residuals change only which flags fail, never a measurement.
        if name == RESIDUALS and (mode_a == mode_b or not (left and right)):
            allowed = True
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


BINARY = 'openbindings-evaluator-bench'
VARIANTS = ['candidate', 'candidate-alloc', 'baseline-identical', 'baseline-alloc-identical',
            'baseline', 'baseline-alloc']
INSTRUMENT_SOURCES = ['src', 'Cargo.toml', 'Cargo.lock', 'build.rs']
SDK_SOURCES = ['crates', 'vendor']
BASELINE_FIXTURES = 'baseline-fixtures.json'
COMPARED = 'compared'
OUTCOME_CHANGED = 'outcome changed by this change'
CANDIDATE_ONLY = 'only in candidate fixtures'


def source_fixtures(source):
    """The rows a source tree's own fixture generator produces, or None without one.

    The generator runs in an isolated interpreter whose only local import path
    is that tree's harness directory, so it never resolves against another
    tree's modules, and it writes no bytecode into the tree.
    """
    directory = source/'tools/evaluator-bench'
    if not (directory/'fixtures.py').is_file():
        return None
    program = ('import json, sys; sys.path.insert(0, sys.argv[1]); '
               'from fixtures import fixtures; print(json.dumps(fixtures()))')
    run = subprocess.run([sys.executable, '-I', '-B', '-c', program, str(directory)],
                         capture_output=True, text=True)
    if run.returncode:
        raise RuntimeError(f'{directory}/fixtures.py did not produce fixtures: {run.stderr}')
    return json.loads(run.stdout)


def workload_comparison(name, fixture, baseline_fixtures):
    """How one candidate workload is compared, and the baseline expectation if it changed.

    Without the merge base's fixtures every workload uses the candidate's
    expectations. Otherwise an expectation that differs between the two
    fixture sets declares an intended outcome change.
    """
    if baseline_fixtures is None:
        return COMPARED, None
    if name not in baseline_fixtures:
        return CANDIDATE_ONLY, None
    # Compare JSON values, so tuple/list spellings cannot hide or invent a change.
    before = baseline_fixtures[name].get('expected')
    if json.loads(json.dumps(fixture.get('expected'))) != before:
        return OUTCOME_CHANGED, before
    return COMPARED, None


def tree_digest(root, entries):
    """The source digest that the instrument's build.rs embeds (FNV-1a, 64-bit).

    Regular files under `entries`, sorted by relative path, each contribute the
    path's UTF-8 bytes, a zero byte, the length as eight little-endian bytes and
    the contents. Symbolic links and anything named `target` are skipped.
    """
    files = []
    def collect(path):
        if path.is_symlink():
            return
        if path.is_dir():
            for child in path.iterdir():
                if child.name != 'target':
                    collect(child)
        elif path.is_file():
            files.append((path.relative_to(root).as_posix(), path))
    for entry in entries:
        if (root/entry).exists():
            collect(root/entry)
    value = 0xcbf29ce484222325
    for relative, path in sorted(files):
        contents = path.read_bytes()
        for chunk in (relative.encode(), b'\0', len(contents).to_bytes(8, 'little'), contents):
            for byte in chunk:
                value = ((value ^ byte) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f'{value:016x}'


def reported_sources(binary):
    """Source digests a built instrument embedded at compile time."""
    run = subprocess.run([str(binary.resolve()), '--provenance'], capture_output=True, text=True)
    if run.returncode:
        raise RuntimeError(f'{binary} could not report its sources: {run.stderr}')
    return json.loads(run.stdout)


def build_instruments(baseline_source, candidate_source, output, binaries, target):
    """Try the identical candidate instrument first; audit before any fallback.

    Every variant builds in its own new target directory. Cargo fingerprints
    the instrument by paths relative to its own workspace and judges sources
    with older modification times fresh, so a shared directory could hand one
    variant another's binary. Each binary must then report the instrument and
    SDK source digests of the trees it was built from.
    """
    reused = [name for name in VARIANTS if (target/name).exists()]
    if reused:
        raise RuntimeError(f'Refusing to reuse target directories {reused} under {target}; pass a new --target')
    output.mkdir(parents=True, exist_ok=True)
    binaries.mkdir(parents=True, exist_ok=True)
    # The measuring step checks the baseline binary against the merge base's
    # own expectations wherever this change declares a different outcome.
    baseline_rows = source_fixtures(baseline_source)
    fixture_path = output/BASELINE_FIXTURES
    if baseline_rows is None:
        fixture_path.unlink(missing_ok=True)
    else:
        fixture_path.write_text(json.dumps(baseline_rows, indent=2)+'\n')
    base = baseline_source/'tools/evaluator-bench'
    candidate = candidate_source/'tools/evaluator-bench'
    candidate_rows = source_fixtures(candidate_source)
    same_requests = (baseline_rows is not None and candidate_rows is not None
                     and requests(baseline_rows) == requests(candidate_rows))
    audit = instrument_diff(base, candidate, sdk_crates(baseline_source, candidate_source), same_requests)
    (output/'instrument.diff').write_text(audit.pop('diff'))
    metadata = dict(kind='identical', instrument_diff=audit,
                    baseline_source=str(baseline_source.resolve()), candidate_source=str(candidate_source.resolve()))
    metadata['baseline_fixtures_sha256'] = None if baseline_rows is None else digest(fixture_path)
    metadata['lock_sha256'] = dict(baseline=digest(base/'Cargo.lock'), candidate=digest(candidate/'Cargo.lock'))
    metadata['builds'] = {}
    roots = [baseline_source, candidate_source]
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
        sdk_digests = {}
        def build(source, root, variant, allocation=False, attempt=''):
            command = ['cargo', 'build', '--locked', '--release', '--manifest-path', str(source/'Cargo.toml')]
            if allocation: command += ['--features', 'allocations']
            suffix = '-alloc' if allocation else ''
            name = variant+suffix+attempt
            directory = target/name
            env = dict(os.environ, CARGO_TARGET_DIR=str(directory.resolve()))
            with (output/f'build-{name}.log').open('w') as log:
                code = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, env=env).returncode
            if code == 0:
                built = directory/'release'/BINARY
                for tree in roots:
                    if tree not in sdk_digests: sdk_digests[tree] = tree_digest(tree, SDK_SOURCES)
                expected = dict(instrument=tree_digest(source, INSTRUMENT_SOURCES), sdk=sdk_digests[root])
                reported = reported_sources(built)
                if reported != expected:
                    other = [tree for tree in roots if tree != root and sdk_digests[tree] == reported.get('sdk')]
                    origin = f'; its SDK sources are those of {other[0]}' if other and expected['sdk'] != reported.get('sdk') else ''
                    raise RuntimeError(f'{name} binary was not built from {source} and {root}: it reports '
                                       f'{reported}, expected {expected}{origin}')
                metadata['builds'][name] = dict(target=str(directory.resolve()), source=str(root.resolve()),
                                                instrument=str(source.resolve()), instrument_digest=expected['instrument'],
                                                sdk_digest=expected['sdk'], binary_sha256=digest(built))
                shutil.copy2(built, binaries/(variant+suffix))
            return code == 0
        # Candidate failures are real build failures, never "not comparable".
        if not build(candidate, candidate_source, 'candidate') or not build(candidate, candidate_source, 'candidate', True):
            raise RuntimeError('Candidate release instrument failed; inspect build logs')
        baseline_builds = build(same, baseline_source, 'baseline', attempt='-identical')
        if baseline_builds:
            baseline_builds = build(same, baseline_source, 'baseline', True, attempt='-identical')
        if not baseline_builds:
            if not audit['confined']:
                metadata['kind'] = 'not comparable'
                metadata['reason'] = ('Candidate instrument failed against merge base; instrument diff is not confined to '
                                      'src/sdk.rs, SDK-owned lock entries and fixture expectations.')
            else:
                metadata['kind'] = 'cross-API'
                if not build(base, baseline_source, 'baseline') or not build(base, baseline_source, 'baseline', True):
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


def unaccepted_flags(row):
    """The memory metrics the policy flags on a row that no declared residual accepts."""
    return [metric for metric in row.get('memory_flags', []) if metric not in row.get('accepted_residuals', [])]


def enforcement_fails(report):
    return bool(report['errors']) or any(unaccepted_flags(row) for row in report['rows'])


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
    instrument_pair = json.loads(pair_path.read_text(encoding='utf-8')) if pair_path.exists() else dict(kind='identical')
    if instrument_pair['kind'] == 'not comparable':
        raise SystemExit('Refusing to measure a pair marked not comparable')
    # The build driver saves the merge base's fixtures beside its pair record.
    baseline_path = args.output/BASELINE_FIXTURES
    baseline_fixtures = json.loads(baseline_path.read_text(encoding='utf-8')) if baseline_path.exists() else None
    baseline_sha256 = digest(baseline_path) if baseline_fixtures is not None else None
    if instrument_pair.get('baseline_fixtures_sha256', baseline_sha256) != baseline_sha256:
        raise SystemExit(f'{baseline_path} is not the merge-base fixture set the build recorded')
    try:
        residuals = accepted_residuals(ACCEPTED_RESIDUALS)
    except ValueError as error:
        raise SystemExit(f'Refusing malformed accepted residuals: {error}')
    candidate_fixtures = fixtures()
    selected = candidate_fixtures
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
    report['baseline_fixtures_sha256'] = baseline_sha256
    report['baseline_only_workloads'] = sorted(set(baseline_fixtures or ()) - set(candidate_fixtures))
    report['accepted_residuals'] = dict(
        path=str(ACCEPTED_RESIDUALS), sha256=digest(ACCEPTED_RESIDUALS) if ACCEPTED_RESIDUALS.exists() else None,
        entries=[dict(entry, status='pending') for entry in residuals])
    for command in [['rustc','-Vv'], ['cargo','-V']]:
        run = subprocess.run(command, capture_output=True, text=True)
        report[command[0]] = run.stdout
    with (args.output/'commands.jsonl').open('w') as log:
        report['empty_process'] = {key:[invoke(getattr(args,key), dict(mode='empty-process',iterations=1), log, rss=True) for _ in range(3)]
                                   for key in ['baseline','candidate','baseline_alloc','candidate_alloc']}
        for name, fixture in selected.items():
            request = {key:value for key,value in fixture.items() if key != 'modes'}
            comparison, baseline_expected = workload_comparison(name, fixture, baseline_fixtures)
            # Both binaries run the candidate's workload. A changed expectation
            # replaces only what the baseline binary is checked against.
            requests = dict(baseline=request, candidate=request)
            if comparison == OUTCOME_CHANGED:
                requests['baseline'] = dict(request, expected=baseline_expected)
            for mode in fixture['modes']:
                row = dict(workload=name, mode=mode, comparison=comparison, samples=[], memory_samples=[], batches=[])
                if comparison == OUTCOME_CHANGED:
                    row['expected'] = dict(baseline=baseline_expected, candidate=fixture.get('expected'))
                report['rows'].append(row)
                try:
                    def run(variant, n, allocation=False, rss=False):
                        binary = getattr(args,variant+('_alloc' if allocation else ''))
                        return invoke(binary, dict(requests[variant], mode=mode, iterations=n), log, rss)
                    if comparison != COMPARED:
                        # Each binary aborts unless it meets its own side's
                        # expectation. Differing outcomes have no comparable
                        # cost, so these rows make no timing or memory decision.
                        sides = ['baseline','candidate'] if comparison == OUTCOME_CHANGED else ['candidate']
                        row['observations'] = {v:run(v, 1)['observation'] for v in sides}
                        print(name,mode,comparison,flush=True)
                        continue
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
                    row['memory_flags'] = [key for key in HEAP_METRICS if memory_regression(row['memory'][key],1.10,128*1024)]
                    if memory_regression(row['memory'][ALLOCATION_METRIC],1.05,32):
                        row['memory_flags'].append(ALLOCATION_METRIC)
                    row['heap_regression'] = any(key in row['memory_flags'] for key in HEAP_METRICS)
                    row['allocation_regression'] = ALLOCATION_METRIC in row['memory_flags']
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
                finally:
                    result_path.write_text(json.dumps(report,indent=2)+'\n')
    report['accepted_residuals']['entries'] = apply_residuals(report, residuals, selected, candidate_fixtures)
    result_path.write_text(json.dumps(report,indent=2)+'\n')
    failures = enforcement_fails(report)
    (args.output/'summary.md').write_text(summary_markdown(report, instrument_pair, args.enforce, baseline_fixtures is not None))
    if args.enforce and failures: raise SystemExit(1)


def summary_markdown(report, instrument_pair, enforce, has_baseline_fixtures):
    def cell(value):
        return '`'+json.dumps(value, separators=(',', ':')).replace('|', '\\|')+'`'
    def line(r):
        if 'error' not in r and r['comparison'] != COMPARED:
            return f"| {r['workload']} | {r['mode']} | n/a | {r['comparison']}; not timed | n/a |\n"
        if r['batches']:
            flags = r.get('memory_flags', [])
            flag = str(bool(unaccepted_flags(r))) if not flags or unaccepted_flags(r) else 'accepted residual'
            timing = r.get('time_decision','error')
            if 'time' in r.get('accepted_residuals', []): timing += ' (accepted residual)'
            return f"| {r['workload']} | {r['mode']} | {r['batches'][-1]['ratio']:.3f} | {timing} | {flag} |\n"
        return f"| {r['workload']} | {r['mode']} | n/a | error | n/a |\n"
    text = '# Evaluator performance\n\nPair: '+instrument_pair['kind']+'.\n\n'
    text += ('Advisory run: outcome errors and heap/count flags are reported without failing.\n\n' if not enforce
             else 'Outcome checks and requested heap/allocation-count thresholds are enforced. Timing and RSS remain advisory.\n\n')
    if not has_baseline_fixtures:
        text += 'Merge-base fixtures were unavailable; both binaries are checked against the candidate\'s expectations.\n\n'
    residuals = report.get('accepted_residuals', {}).get('entries', [])
    if residuals:
        text += ('## Accepted residuals\n\nThis change declares these regressions accepted in `'+RESIDUALS+'`. '
                 'An accepted metric passes within its bound; an exceeded bound fails like any other flag (time '
                 'stays advisory). Remove a stale entry: its row no longer exists, is not compared or no longer '
                 'regresses.\n\n| Workload | Mode | Metric | Candidate/base | Bound | Status | Reason |\n'
                 '| --- | --- | --- | ---: | ---: | --- | --- |\n')
        for entry in residuals:
            ratio = 'n/a' if entry.get('ratio') is None else f"{entry['ratio']:.4f}"
            reason = entry['reason'].replace('|', '\\|')
            text += (f"| {entry['workload']} | {entry['mode']} | {entry['metric']} | {ratio} | {entry['max_ratio']} | "
                     f"**{entry['status']}** ({entry.get('detail', '')}) | {reason} |\n")
        text += '\n'
    text += '| Workload | Mode | Candidate/base | Time decision | Heap/count flag |\n| --- | --- | ---: | --- | --- |\n'
    text += ''.join(line(r) for r in report['rows'])
    changed = [r for r in report['rows'] if r['comparison'] == OUTCOME_CHANGED]
    if changed:
        text += ('\n## Outcomes changed by this change\n\nThe merge base and the candidate expect different outcomes. '
                 'Each binary is checked against its own side\'s expectation; these rows are excluded from timing '
                 'and memory decisions.\n\n| Workload | Mode | Baseline expectation | Candidate expectation |\n'
                 '| --- | --- | --- | --- |\n')
        text += ''.join(f"| {r['workload']} | {r['mode']} | {cell(r['expected']['baseline'])} | {cell(r['expected']['candidate'])} |\n"
                        for r in changed)
    candidate_only = sorted({r['workload'] for r in report['rows'] if r['comparison'] == CANDIDATE_ONLY})
    if candidate_only or report['baseline_only_workloads']:
        text += '\n## Workloads in one fixture set only\n\nReported, not compared. Candidate-only workloads are checked against the candidate\'s expectations.\n\n'
        text += ''.join(f'- `{name}`: candidate only\n' for name in candidate_only)
        text += ''.join(f'- `{name}`: merge base only\n' for name in report['baseline_only_workloads'])
    return text

if __name__ == '__main__':
    if sys.argv[1:2] == ['build']:
        build_main()
    else:
        main()
