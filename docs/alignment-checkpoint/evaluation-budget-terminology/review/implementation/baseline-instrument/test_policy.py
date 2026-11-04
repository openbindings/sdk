"""Exercise enforced outcomes/memory and advisory timing through the comparison driver."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import shutil
from fixtures import fixtures
import compare
from compare import confirm, memory_regression, time_decision

class Policy(unittest.TestCase):
    def row(self, **overrides):
        return dict(ratio=1.3, difference=100, ci95=[1.25,1.35], base_mad_fraction=.02, candidate_mad_fraction=.02, **overrides)
    def test_confirmation_and_noise(self):
        row=self.row()
        self.assertEqual(time_decision(row,1.2,20),'regression')
        row['base_mad_fraction']=.11
        self.assertEqual(time_decision(row,1.2,20),'inconclusive')
        self.assertEqual(confirm(['regression','regression']),'regression')
        for decisions in [['regression'],['regression','within-threshold'],['inconclusive','inconclusive']]:
            self.assertEqual(confirm(decisions),'inconclusive')
    def test_interval_and_absolute_floor(self):
        row=self.row();row['ci95']=[1.15,1.4]
        self.assertEqual(time_decision(row,1.2,20),'inconclusive')
        row['difference']=19
        self.assertEqual(time_decision(row,1.2,20),'within-threshold')
    def test_memory_requires_both_thresholds(self):
        self.assertFalse(memory_regression(dict(base=0,candidate=20,difference=20),1.05,32))
        self.assertFalse(memory_regression(dict(base=10000,candidate=10040,difference=40),1.05,32))
        self.assertTrue(memory_regression(dict(base=100,candidate=140,difference=40),1.05,32))

    def test_memory_thresholds_are_strict(self):
        for relative, absolute, base in [(1.05, 32, 1000), (1.10, 128*1024, 2*1024*1024)]:
            for difference in [absolute, base*(relative-1)]:
                self.assertFalse(memory_regression(
                    dict(base=base,candidate=base+difference,difference=difference), relative, absolute))
            difference = max(absolute, base*(relative-1)) + 1
            self.assertTrue(memory_regression(
                dict(base=base,candidate=base+difference,difference=difference), relative, absolute))

    def run_comparison(self, *, enforce, metric=None, timing='within-threshold', cross_api=False,
                       factor=2, residuals=None, rows=None, workloads=None, summary=False):
        """Feed deterministic instrument observations through the real report/exit path.

        `metric` names one memory metric, or a list of them, that the candidate
        multiplies by `factor`; `residuals` is the accepted-residuals document
        the change declares (none by default).
        """
        metrics = [metric] if isinstance(metric, str) else list(metric or [])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arguments = ['compare.py', '--output', str(root/'output')]
            if workloads:
                arguments.extend(['--workloads', workloads])
            declared = root/'accepted-residuals.json'
            if residuals is not None:
                declared.write_text(residuals if isinstance(residuals, str) else json.dumps(residuals))
            if cross_api:
                (root/'output').mkdir()
                (root/'output/instrument-pair.json').write_text(json.dumps(dict(kind='cross-API')))

            for key in ['baseline', 'candidate', 'baseline-alloc', 'candidate-alloc']:
                binary = root/key
                binary.write_text(key)
                arguments.extend(['--'+key, str(binary)])
            if enforce:
                arguments.append('--enforce')

            def invoke(binary, request, log, rss=False):
                memory = {key: 1024*1024 for key in [
                    'baseline_live_bytes', 'peak_live_bytes', 'incremental_peak_bytes',
                    'requested_bytes_per_op']}
                memory['allocations_per_op'] = 100
                if binary.name.startswith('candidate'):
                    for name in metrics:
                        memory[name] *= factor
                return dict(iterations=request['iterations'],
                            outcomes={'satisfies': request['iterations']},
                            observation={'status': 'satisfies'},
                            nanoseconds=request['iterations']*200_000_000,
                            memory=memory,
                            rss_bytes=8*1024*1024 if binary.name.startswith('candidate') else 1024*1024)

            with patch.object(sys, 'argv', arguments), \
                 patch.object(compare, 'fixtures', return_value=rows or {'scalar': {'modes': ['eval']}}), \
                 patch.object(compare, 'invoke', side_effect=invoke), \
                 patch.object(compare, 'time_decision', return_value=timing), \
                 patch.object(compare, 'ACCEPTED_RESIDUALS', declared):
                try:
                    compare.main()
                    exit_code = 0
                except SystemExit as error:
                    exit_code = error.code
            report = json.loads((root/'output/comparison.json').read_text(encoding='utf-8'))
            if summary:
                return exit_code, report, (root/'output/summary.md').read_text(encoding='utf-8')
            return exit_code, report

    def test_heap_and_count_flags_fail_only_enforced_runs(self):
        for metric in ['baseline_live_bytes', 'peak_live_bytes', 'incremental_peak_bytes',
                       'requested_bytes_per_op', 'allocations_per_op']:
            for enforce in [False, True]:
                with self.subTest(metric=metric, enforce=enforce):
                    code, report = self.run_comparison(enforce=enforce, metric=metric)
                    self.assertEqual(code, int(enforce))
                    self.assertEqual(report['errors'], [])
                    flag = 'allocation_regression' if metric == 'allocations_per_op' else 'heap_regression'
                    self.assertTrue(report['rows'][0][flag])
                    self.assertEqual(report['advisory'], not enforce)

    def test_timing_and_rss_remain_advisory_under_enforcement(self):
        for timing in ['within-threshold', 'regression', 'inconclusive']:
            with self.subTest(timing=timing):
                code, report = self.run_comparison(enforce=True, timing=timing)
                self.assertEqual(code, 0)
                self.assertEqual(report['rows'][0]['time_decision'], timing)
                self.assertTrue(report['rows'][0]['rss_needs_verification'])
                self.assertTrue(report['timing_advisory'])

    def test_cross_api_pairs_use_the_same_enforcement_and_timing_policy(self):
        code, report = self.run_comparison(enforce=True, metric='allocations_per_op',
                                           timing='regression', cross_api=True)
        self.assertEqual(code, 1)
        self.assertEqual(report['instrument_pair']['kind'], 'cross-API')
        self.assertTrue(report['rows'][0]['allocation_regression'])
        self.assertEqual(report['rows'][0]['time_decision'], 'regression')

    def test_advisory_records_mismatch_and_enforcement_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binaries = []
            for name, status in [('base', 'satisfies'), ('candidate', 'fails')]:
                binary = root/name
                binary.write_text(f"#!{sys.executable}\n" +
                    "import json,sys\n" +
                    "empty = '--empty' in sys.argv\n" +
                    "request = {} if empty else json.load(sys.stdin)\n" +
                    f"status = 'empty-process' if empty else {status!r}\n" +
                    "n=request.get('iterations',1)\n" +
                    "print(json.dumps(dict(iterations=n,observation=dict(status=status),outcomes={status:n})))\n")
                binary.chmod(0o755)
                binaries.append(binary)
            for enforce, expected_exit in [(False, 0), (True, 1)]:
                output = root/('enforced' if enforce else 'advisory')
                command = [sys.executable, str(Path(__file__).with_name('compare.py')),
                           '--baseline', str(binaries[0]), '--candidate', str(binaries[1]),
                           '--baseline-alloc', str(binaries[0]), '--candidate-alloc', str(binaries[1]),
                           '--output', str(output), '--workloads', 'scalar']
                if enforce: command.append('--enforce')
                result = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected_exit, result.stderr)
                report = json.loads((output/'comparison.json').read_text(encoding='utf-8'))
                self.assertEqual(len(report['errors']), 7)
                self.assertEqual(report['advisory'], not enforce)
                self.assertTrue(report['timing_advisory'])

class AcceptedResiduals(unittest.TestCase):
    """A change may declare an exact regression accepted, within a bound, for a named reason."""
    REASON = 'Matthew, 2026-10-20: accepted cost of a schema transform'

    def document(self, *entries):
        return dict(format=compare.RESIDUAL_FORMAT, residuals=[
            dict(workload=workload, mode=mode, metric=metric, max_ratio=bound, reason=self.REASON)
            for workload, mode, metric, bound in entries])

    def run_comparison(self, **options):
        return Policy.run_comparison(self, enforce=True, summary=True, **options)

    def statuses(self, report):
        return [(entry['workload'], entry['mode'], entry['metric'], entry['status'])
                for entry in report['accepted_residuals']['entries']]

    def test_a_declared_residual_within_its_bound_passes_and_is_reported(self):
        for metric in ['incremental_peak_bytes', 'requested_bytes_per_op', 'allocations_per_op']:
            with self.subTest(metric=metric):
                code, report, summary = self.run_comparison(
                    metric=metric, residuals=self.document(('scalar', 'eval', metric, 2.01)))
                self.assertEqual(code, 0)
                self.assertEqual(self.statuses(report), [('scalar', 'eval', metric, 'accepted')])
                self.assertEqual(report['accepted_residuals']['entries'][0]['ratio'], 2.0)
                self.assertEqual(report['rows'][0]['memory_flags'], [metric])
                self.assertEqual(report['rows'][0]['accepted_residuals'], [metric])
                self.assertIsNotNone(report['accepted_residuals']['sha256'])
                self.assertIn('## Accepted residuals', summary)
                self.assertIn(self.REASON, summary)
                self.assertIn('| scalar | eval | 1.000 | within-threshold | accepted residual |', summary)
                # The section precedes the per-row table, so a reviewer sees it first.
                self.assertLess(summary.index('## Accepted residuals'), summary.index('| Workload | Mode | Candidate/base | Time'))

    def test_a_residual_beyond_its_bound_still_fails(self):
        code, report, summary = self.run_comparison(
            metric='incremental_peak_bytes', residuals=self.document(('scalar', 'eval', 'incremental_peak_bytes', 1.99)))
        self.assertEqual(code, 1)
        self.assertEqual(self.statuses(report), [('scalar', 'eval', 'incremental_peak_bytes', 'exceeded')])
        self.assertNotIn('accepted_residuals', report['rows'][0])
        self.assertIn('**exceeded**', summary)
        self.assertIn('| scalar | eval | 1.000 | within-threshold | True |', summary)

    def test_undeclared_regressions_still_fail(self):
        # Another metric on the declared row.
        code, report, _ = self.run_comparison(
            metric=['incremental_peak_bytes', 'allocations_per_op'],
            residuals=self.document(('scalar', 'eval', 'incremental_peak_bytes', 2.01)))
        self.assertEqual(code, 1)
        self.assertEqual(self.statuses(report), [('scalar', 'eval', 'incremental_peak_bytes', 'accepted')])
        self.assertEqual(compare.unaccepted_flags(report['rows'][0]), ['allocations_per_op'])
        # Another row of the declared workload.
        rows = {'scalar': {'modes': ['eval', 'compile']}}
        code, report, _ = self.run_comparison(
            metric='allocations_per_op', rows=rows,
            residuals=self.document(('scalar', 'eval', 'allocations_per_op', 2.01)))
        self.assertEqual(code, 1)
        self.assertEqual([compare.unaccepted_flags(row) for row in report['rows']], [[], ['allocations_per_op']])
        # No declaration at all.
        code, report, summary = self.run_comparison(metric='allocations_per_op')
        self.assertEqual(code, 1)
        self.assertEqual(report['accepted_residuals']['entries'], [])
        self.assertNotIn('## Accepted residuals', summary)

    def test_stale_entries_are_reported_and_accept_nothing(self):
        code, report, summary = self.run_comparison(residuals=self.document(
            ('absent', 'eval', 'allocations_per_op', 1.1),
            ('scalar', 'compile', 'allocations_per_op', 1.1),
            ('scalar', 'eval', 'allocations_per_op', 1.1),
            ('scalar', 'eval', 'time', 1.1)))
        self.assertEqual(code, 0)
        entries = report['accepted_residuals']['entries']
        self.assertEqual([entry['status'] for entry in entries], ['stale'] * 4)
        self.assertEqual([entry['detail'] for entry in entries], [
            'no such workload and mode', 'no such workload and mode',
            'the policy no longer flags this metric', 'the policy no longer flags this metric'])
        self.assertNotIn('accepted_residuals', report['rows'][0])
        self.assertEqual(summary.count('**stale**'), 4)
        # A stale entry never hides a regression.
        code, report, _ = self.run_comparison(
            metric='allocations_per_op', residuals=self.document(('absent', 'eval', 'allocations_per_op', 9.0)))
        self.assertEqual(code, 1)

    def test_rows_not_compared_or_not_selected_accept_nothing(self):
        rows = {'scalar': {'modes': ['eval']}, 'other': {'modes': ['eval']}}
        code, report, _ = self.run_comparison(
            rows=rows, workloads='scalar', residuals=self.document(('other', 'eval', 'allocations_per_op', 1.1)))
        self.assertEqual(code, 0)
        self.assertEqual(self.statuses(report), [('other', 'eval', 'allocations_per_op', 'not measured')])
        row = dict(workload='w', mode='eval', comparison=compare.OUTCOME_CHANGED)
        judged = compare.apply_residuals(dict(rows=[row]), self.document(('w', 'eval', 'time', 1.1))['residuals'],
                                         {'w': {'modes': ['eval']}}, {'w': {'modes': ['eval']}})
        self.assertEqual(judged[0]['status'], 'stale')
        judged = compare.apply_residuals(dict(rows=[dict(row, comparison=compare.COMPARED, error='boom')]),
                                         self.document(('w', 'eval', 'time', 1.1))['residuals'],
                                         {'w': {'modes': ['eval']}}, {'w': {'modes': ['eval']}})
        self.assertEqual(judged[0]['status'], 'error')

    def test_time_residuals_are_reported_and_timing_stays_advisory(self):
        code, report, summary = self.run_comparison(
            timing='regression', residuals=self.document(('scalar', 'eval', 'time', 1.01)))
        self.assertEqual(code, 0)
        self.assertEqual(self.statuses(report), [('scalar', 'eval', 'time', 'accepted')])
        self.assertIn('| scalar | eval | 1.000 | regression (accepted residual) | False |', summary)
        with patch.object(compare, 'residual_ratio', return_value=1.5):
            code, report, _ = self.run_comparison(
                timing='regression', residuals=self.document(('scalar', 'eval', 'time', 1.01)))
        self.assertEqual(code, 0)
        self.assertEqual(self.statuses(report), [('scalar', 'eval', 'time', 'exceeded')])

    def test_malformed_residuals_are_refused_before_measuring(self):
        entry = dict(workload='scalar', mode='eval', metric='allocations_per_op', max_ratio=1.1, reason=self.REASON)
        def document(**changes):
            return json.dumps(dict(format=compare.RESIDUAL_FORMAT, residuals=[dict(entry, **changes)]))
        cases = {
            'not JSON': '{',
            'wrong format': json.dumps(dict(format='other', residuals=[])),
            'extra top-level member': json.dumps(dict(format=compare.RESIDUAL_FORMAT, residuals=[], note='x')),
            'residuals not an array': json.dumps(dict(format=compare.RESIDUAL_FORMAT, residuals={})),
            'extra entry member': json.dumps(dict(format=compare.RESIDUAL_FORMAT, residuals=[dict(entry, wildcard=True)])),
            'unknown metric': document(metric='rss'),
            'empty reason': document(reason=' '),
            'workload not a string': document(workload=['scalar']),
            'bound of one': document(max_ratio=1),
            'boolean bound': document(max_ratio=True),
            'infinite bound': document(max_ratio=float('inf')),
            'repeated entry': json.dumps(dict(format=compare.RESIDUAL_FORMAT, residuals=[entry, entry])),
        }
        for name, text in cases.items():
            with self.subTest(case=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root/'accepted-residuals.json').write_text(text)
                arguments = ['compare.py', '--output', str(root/'output'), '--enforce']
                for key in ['baseline', 'candidate', 'baseline-alloc', 'candidate-alloc']:
                    (root/key).write_text(key)
                    arguments.extend(['--'+key, str(root/key)])
                with patch.object(sys, 'argv', arguments), \
                     patch.object(compare, 'ACCEPTED_RESIDUALS', root/'accepted-residuals.json'), \
                     patch.object(compare, 'invoke', side_effect=AssertionError('measured')):
                    with self.assertRaises(SystemExit) as raised:
                        compare.main()
                self.assertIn('Refusing malformed accepted residuals', str(raised.exception.code))
                self.assertFalse((root/'output/comparison.json').exists())

    def test_the_checked_in_residuals_are_valid_and_name_existing_rows(self):
        rows = fixtures()
        for entry in compare.accepted_residuals(compare.ACCEPTED_RESIDUALS):
            self.assertIn(entry['mode'], rows[entry['workload']]['modes'], entry)


class OutcomeChanges(unittest.TestCase):
    """An expectation that differs from the merge base's declares an intended outcome change."""
    @staticmethod
    def workload(value, **expected):
        return dict(schema='true', value=value, expected=expected, modes=['eval'])

    def compare(self, candidate, baseline, observed, pair=None):
        """Run the enforced driver; fake binaries check their request's expectation as the instrument does."""
        calls = []
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root/'output'
            output.mkdir()
            if baseline is not None:
                (output/compare.BASELINE_FIXTURES).write_text(json.dumps(baseline))
            if pair is not None:
                (output/'instrument-pair.json').write_text(json.dumps(pair))
            arguments = ['compare.py', '--output', str(output), '--enforce']
            for key in ['baseline', 'candidate', 'baseline-alloc', 'candidate-alloc']:
                (root/key).write_text(key)
                arguments.extend(['--'+key, str(root/key)])

            def invoke(binary, request, log, rss=False):
                side = 'candidate' if binary.name.startswith('candidate') else 'baseline'
                n = request['iterations']
                if request['mode'] == 'empty-process':
                    observation = dict(status='empty-process')
                else:
                    observation = observed[side][request['value']]
                    calls.append((side, request['value'], n, request['expected']))
                    for key, value in request['expected'].items():
                        if observation.get(key) != value:
                            raise RuntimeError(f'{binary.name} exited 101: unexpected {key}: {observation}')
                memory = {key: 1024 for key in ['baseline_live_bytes', 'peak_live_bytes', 'incremental_peak_bytes',
                                                'requested_bytes_per_op', 'allocations_per_op']}
                return dict(iterations=n, outcomes={observation['status']: n}, observation=observation,
                            nanoseconds=n*200_000_000, memory=memory, rss_bytes=1024*1024)

            with patch.object(sys, 'argv', arguments), \
                 patch.object(compare, 'fixtures', return_value=candidate), \
                 patch.object(compare, 'invoke', side_effect=invoke):
                try:
                    compare.main()
                    code = 0
                except SystemExit as error:
                    code = error.code
            def read(name):
                path = output/name
                return path.read_text(encoding='utf-8') if path.exists() else None
            report = read('comparison.json')
            return code, report and json.loads(report), read('summary.md'), calls

    def test_a_changed_expectation_is_checked_per_side_and_excluded_from_decisions(self):
        old, new = dict(status='no-verdict', code='work'), dict(status='no-verdict', code='depth')
        candidate = dict(stop=self.workload('stop', **new), same=self.workload('same', status='satisfies'))
        baseline = dict(stop=self.workload('stop', **old), same=self.workload('same', status='satisfies'))
        observed = dict(baseline=dict(stop=old, same=dict(status='satisfies')),
                        candidate=dict(stop=new, same=dict(status='satisfies')))
        code, report, summary, calls = self.compare(candidate, baseline, observed)
        self.assertEqual(code, 0)
        self.assertEqual(report['errors'], [])
        stop, same = report['rows']
        self.assertEqual(stop['comparison'], compare.OUTCOME_CHANGED)
        self.assertEqual(stop['expected'], dict(baseline=old, candidate=new))
        self.assertEqual(stop['observations'], dict(baseline=old, candidate=new))
        self.assertEqual(stop['batches'], [])
        for key in ['time_decision', 'memory', 'heap_regression', 'allocation_regression', 'rss']:
            self.assertNotIn(key, stop)
        # One untimed check per binary, each against its own side's expectation.
        self.assertEqual([call for call in calls if call[1] == 'stop'],
                         [('baseline', 'stop', 1, old), ('candidate', 'stop', 1, new)])
        # Every other row is compared exactly as before.
        self.assertEqual(same['comparison'], compare.COMPARED)
        self.assertEqual(same['observation'], dict(status='satisfies'))
        self.assertEqual(same['time_decision'], 'within-threshold')
        self.assertFalse(same['heap_regression'] or same['allocation_regression'])
        self.assertIn('| stop | eval | n/a | outcome changed by this change; not timed | n/a |', summary)
        self.assertIn('| stop | eval | `{"status":"no-verdict","code":"work"}` | '
                      '`{"status":"no-verdict","code":"depth"}` |', summary)

    def test_an_unchanged_expectation_still_requires_identical_outcomes(self):
        expected = dict(status='no-verdict', code='work')
        fixture = dict(stop=self.workload('stop', **expected))
        for case, before, after in [
                ('candidate misses the shared expectation', expected, dict(status='no-verdict', code='depth')),
                ('only an undeclared field differs', dict(expected, detail='old'), dict(expected, detail='new'))]:
            with self.subTest(case):
                code, report, summary, _ = self.compare(fixture, fixture,
                                                        dict(baseline=dict(stop=before), candidate=dict(stop=after)))
                self.assertEqual(code, 1)
                self.assertEqual(report['rows'][0]['comparison'], compare.COMPARED)
                self.assertEqual([error['workload'] for error in report['errors']], ['stop'])
                self.assertIn('| stop | eval | n/a | error | n/a |', summary)

    def test_a_changed_expectation_still_requires_each_side_to_meet_its_own(self):
        old, new = dict(status='no-verdict', code='work'), dict(status='no-verdict', code='depth')
        candidate, baseline = dict(stop=self.workload('stop', **new)), dict(stop=self.workload('stop', **old))
        for case, before, after in [('candidate misses its new expectation', old, old),
                                    ('baseline misses its old expectation', new, new)]:
            with self.subTest(case):
                code, report, summary, _ = self.compare(candidate, baseline,
                                                        dict(baseline=dict(stop=before), candidate=dict(stop=after)))
                self.assertEqual(code, 1)
                self.assertEqual(report['rows'][0]['comparison'], compare.OUTCOME_CHANGED)
                self.assertEqual([error['workload'] for error in report['errors']], ['stop'])
                self.assertIn('| stop | eval | n/a | error | n/a |', summary)

    def test_workloads_in_one_fixture_set_are_reported_not_compared(self):
        candidate = dict(new=self.workload('new', status='satisfies'))
        baseline = dict(removed=self.workload('removed', status='satisfies'))
        code, report, summary, calls = self.compare(candidate, baseline,
                                                    dict(baseline={}, candidate=dict(new=dict(status='satisfies'))))
        self.assertEqual(code, 0)
        self.assertEqual(report['rows'][0]['comparison'], compare.CANDIDATE_ONLY)
        self.assertEqual(report['rows'][0]['batches'], [])
        self.assertEqual(calls, [('candidate', 'new', 1, dict(status='satisfies'))])
        self.assertEqual(report['baseline_only_workloads'], ['removed'])
        self.assertIn('- `new`: candidate only', summary)
        self.assertIn('- `removed`: merge base only', summary)
        # The candidate's own expectation is still enforced.
        code, report, _, _ = self.compare(candidate, baseline,
                                          dict(baseline={}, candidate=dict(new=dict(status='fails'))))
        self.assertEqual(code, 1)
        self.assertEqual([error['workload'] for error in report['errors']], ['new'])

    def test_the_merge_base_fixtures_read_must_be_the_ones_the_build_recorded(self):
        import hashlib
        fixture = dict(same=self.workload('same', status='satisfies'))
        observed = dict(baseline=dict(same=dict(status='satisfies')), candidate=dict(same=dict(status='satisfies')))
        recorded = hashlib.sha256(json.dumps(fixture).encode()).hexdigest()
        code, report, _, _ = self.compare(fixture, fixture, observed,
                                          pair=dict(kind='identical', baseline_fixtures_sha256=recorded))
        self.assertEqual(code, 0)
        self.assertEqual(report['baseline_fixtures_sha256'], recorded)
        for baseline, pair_digest in [(fixture, '0'*64), (fixture, None), (None, recorded)]:
            with self.subTest(recorded=pair_digest, saved=baseline is not None):
                code, report, _, calls = self.compare(fixture, baseline, observed,
                                                      pair=dict(kind='identical', baseline_fixtures_sha256=pair_digest))
                self.assertIn('not the merge-base fixture set', code)
                self.assertIsNone(report)
                self.assertEqual(calls, [])

class InstrumentConfinement(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.base, self.candidate = self.root/'base', self.root/'candidate'
        self.lock = ('version = 4\n\n[[package]]\nname = "openbindings"\nversion = "0.1"\n'
                     'dependencies = ["serde"]\n\n[[package]]\nname = "serde"\nversion = "1.0"\n')
        for root in [self.base, self.candidate]:
            (root/'src').mkdir(parents=True)
            (root/'src/sdk.rs').write_text('old SDK calls\n')
            (root/'src/main.rs').write_text('stable timing\n')
            (root/'Cargo.lock').write_text(self.lock)

    def audit(self):
        return compare.instrument_diff(self.base, self.candidate, {'openbindings'})

    def test_only_sdk_calls_and_sdk_package_lock_entries_may_change(self):
        (self.candidate/'src/sdk.rs').write_text('new SDK calls\n')
        (self.candidate/'Cargo.lock').write_text(self.lock.replace('"0.1"', '"0.2"'))
        audit = self.audit()
        self.assertTrue(audit['confined'])
        self.assertEqual(audit['changed'], ['Cargo.lock', 'src/sdk.rs'])
        import hashlib
        self.assertEqual(audit['sha256'], hashlib.sha256(audit['diff'].encode()).hexdigest())
        self.assertIn('new SDK calls', audit['diff'])

    def test_any_other_instrument_edit_blocks_cross_api_pair(self):
        for name in ['src/main.rs', 'fixtures.py', 'compare.py', 'Cargo.toml', 'README.md', 'src/allocations.rs',
                     'src/target/timer.rs', 'extra.pyc']:
            with self.subTest(name=name):
                path = self.candidate/name
                path.parent.mkdir(parents=True, exist_ok=True)
                original = path.read_bytes() if path.exists() else None
                path.write_text('changed\n')
                self.assertFalse(self.audit()['confined'])
                if original is None: path.unlink()
                else: path.write_bytes(original)

    def test_fixture_edits_may_change_expectations_but_not_requests(self):
        (self.base/'fixtures.py').write_text('old expectations\n')
        (self.candidate/'fixtures.py').write_text('new expectations\n')
        audit = compare.instrument_diff(self.base, self.candidate, {'openbindings'}, same_requests=True)
        self.assertTrue(audit['confined'])
        self.assertEqual(audit['changed'], ['fixtures.py'])
        self.assertFalse(self.audit()['confined'])
        (self.base/'fixtures.py').unlink()
        audit = compare.instrument_diff(self.base, self.candidate, {'openbindings'}, same_requests=True)
        self.assertFalse(audit['confined'])

    def test_declared_residuals_may_change_without_blocking_a_cross_api_pair(self):
        (self.candidate/'src/sdk.rs').write_text('new SDK calls\n')
        residuals = self.candidate/'accepted-residuals.json'
        residuals.write_text('{"format": "x", "residuals": []}\n')
        audit = self.audit()
        self.assertTrue(audit['confined'])
        self.assertEqual(audit['changed'], ['accepted-residuals.json', 'src/sdk.rs'])
        (self.base/'accepted-residuals.json').write_text('{"format": "x", "residuals": [1]}\n')
        self.assertTrue(self.audit()['confined'])
        residuals.unlink()
        self.assertTrue(self.audit()['confined'])
        residuals.symlink_to(self.base/'accepted-residuals.json')
        self.assertFalse(self.audit()['confined'])

    def test_instrument_tests_may_change_without_blocking_a_cross_api_pair(self):
        (self.candidate/'src/sdk.rs').write_text('new SDK calls\n')
        (self.base/'tests').mkdir()
        (self.candidate/'tests').mkdir()
        (self.base/'tests/observations.rs').write_text('old expectation\n')
        observations = self.candidate/'tests/observations.rs'
        observations.write_text('new expectation\n')
        audit = self.audit()
        self.assertTrue(audit['confined'])
        self.assertEqual(audit['changed'], ['src/sdk.rs', 'tests/observations.rs'])
        self.assertNotIn('tests', compare.INSTRUMENT_SOURCES)
        observations.unlink()
        observations.symlink_to(self.base/'tests/observations.rs')
        self.assertFalse(self.audit()['confined'])

    def test_requests_are_fixture_rows_without_expectations(self):
        rows = {'w': {'value': '1', 'modes': ('eval',), 'expected': {'status': 'fails'}}}
        self.assertEqual(compare.requests(rows), {'w': {'value': '1', 'modes': ['eval']}})
        changed = {'w': {'value': '1', 'modes': ['eval'], 'expected': {'status': 'no-verdict'}}}
        self.assertEqual(compare.requests(rows), compare.requests(changed))
        self.assertNotEqual(compare.requests(rows), compare.requests({'w': {'value': '2', 'modes': ['eval']}}))

    def test_third_party_lock_and_metadata_edits_are_not_sdk_edits(self):
        for text in [self.lock.replace('"1.0"', '"2.0"'),
                     self.lock.replace('version = 4', 'version = 3'),
                     self.lock+'\n[[package]]\nname = "other"\nversion = "1"\n',
                     self.lock.replace('[[package]]\nname = "serde"',
                         '[metadata]\nadded = "forbidden"\n\n[[package]]\nname = "serde"'),
                     self.lock.replace('[[package]]\nname = "serde"',
                         '  [metadata]\nadded = "forbidden"\n\n[[package]]\nname = "serde"'),
                     self.lock.replace('[[package]]\nname = "serde"',
                         '[[patch.unused]]\nname = "other"\nversion = "1"\n\n[[package]]\nname = "serde"'),
                     'invalid TOML']:
            (self.candidate/'Cargo.lock').write_text(text)
            self.assertFalse(self.audit()['confined'])

    def test_deletion_and_file_mode_changes_cannot_hide_outside_shim(self):
        (self.candidate/'src/main.rs').unlink()
        self.assertFalse(self.audit()['confined'])
        (self.candidate/'src/main.rs').write_text('stable timing\n')
        (self.candidate/'src/main.rs').chmod(0o755)
        self.assertFalse(self.audit()['confined'])

    def test_symlinked_sources_cannot_hide_extra_instrument_changes(self):
        outside = self.root/'outside'
        outside.mkdir()
        (outside/'timer.rs').write_text('changed timing')
        (self.candidate/'src/extra').symlink_to(outside, target_is_directory=True)
        audit = self.audit()
        self.assertFalse(audit['confined'])
        self.assertIn('src/extra', audit['forbidden'])

    def test_generated_build_and_python_cache_files_are_not_instrument_sources(self):
        for name in ['target/debug/binary', '__pycache__/fixtures.cpython-313.pyc']:
            path = self.candidate/name
            path.parent.mkdir(parents=True)
            path.write_bytes(b'generated')
        self.assertEqual(self.audit()['changed'], [])

    def test_identical_instruments_do_not_authorize_a_failed_build_fallback(self):
        self.assertFalse(self.audit()['confined'])

    def build(self, *, confined=True, fail_candidate=False, fail_own_base=False, identical_builds=False,
              reports=None, fixtures=None):
        # Fake only Cargo and the binaries, retaining the real copying,
        # confinement, fallback and provenance logic. Unless `identical_builds`,
        # the identical instrument fails on the old SDK by construction. Like
        # real Cargo with relative workspace fingerprints and old modification
        # times, the fake judges a directory that already holds a binary fresh
        # and leaves that binary in place. A fake binary reports the digests of
        # the trees it was built from, or `reports(name, sources)` if given.
        sources = []
        for variant, instrument in [('baseline', self.base), ('candidate', self.candidate)]:
            source = self.root/'sources'/variant
            shutil.copytree(instrument, source/'tools/evaluator-bench')
            (source/'crates/core').mkdir(parents=True)
            (source/'crates/core/Cargo.toml').write_text(f'[package]\nname = "openbindings"\n# {variant}\n')
            sources.append(source)
        (sources[1]/'tools/evaluator-bench/src/sdk.rs').write_text('new SDK calls\n')
        for source, rows in zip(sources, fixtures or []):
            (source/'tools/evaluator-bench/fixtures.py').write_text(f'def fixtures():\n    return {rows!r}\n')
        if not confined:
            (sources[1]/'tools/evaluator-bench/src/main.rs').write_text('changed timing\n')
        calls = []
        target = self.root/'target'
        real_run = subprocess.run
        def run(command, **options):
            if command[0] == sys.executable:
                return real_run(command, **options)
            if command[1:] == ['--provenance']:
                return subprocess.CompletedProcess(command, 0, stdout=Path(command[0]).read_text(encoding='utf-8'), stderr='')
            manifest = Path(command[command.index('--manifest-path')+1])
            directory = Path(options['env']['CARGO_TARGET_DIR'])
            calls.append((str(manifest), directory))
            is_candidate = manifest.is_relative_to(sources[1])
            identical = manifest.parent.name.startswith('evaluator-candidate-')
            fails = (identical and not identical_builds) or (is_candidate and fail_candidate) or (not is_candidate and fail_own_base)
            binary = directory/'release'/compare.BINARY
            if not fails and not binary.exists():
                binary.parent.mkdir(parents=True)
                root = sources[1] if is_candidate else sources[0]
                report = dict(instrument=compare.tree_digest(manifest.parent, compare.INSTRUMENT_SOURCES),
                              sdk=compare.tree_digest(root, compare.SDK_SOURCES))
                if reports:
                    report = reports(directory.name, sources) or report
                binary.write_text(json.dumps(report))
            return subprocess.CompletedProcess(command, int(fails))
        with patch.object(compare.subprocess, 'run', side_effect=run):
            metadata = compare.build_instruments(*sources, self.root/'output', self.root/'bin', target)
        return metadata, calls

    def test_each_variant_builds_in_its_own_new_target_directory(self):
        # One shared directory would hand the identical baseline the candidate's
        # binary; separate directories keep every binary its own build.
        metadata, calls = self.build(identical_builds=True)
        self.assertEqual(metadata['kind'], 'identical')
        directories = [directory for _, directory in calls]
        self.assertEqual(directories, [(self.root/'target'/name).resolve() for name in compare.VARIANTS[:4]])
        builds = metadata['builds']
        self.assertEqual(list(builds), compare.VARIANTS[:4])
        for name, tree in [('candidate', 'candidate'), ('baseline-identical', 'baseline')]:
            root = self.root/'sources'/tree
            self.assertEqual(builds[name]['source'], str(root.resolve()))
            self.assertEqual(builds[name]['sdk_digest'], compare.tree_digest(root, compare.SDK_SOURCES))
            self.assertEqual(json.loads((self.root/'bin'/name.removesuffix('-identical')).read_text(encoding='utf-8'))['sdk'],
                             builds[name]['sdk_digest'])
        self.assertTrue(Path(builds['baseline-identical']['instrument']).name.startswith('evaluator-candidate-'))
        self.assertEqual(builds['baseline-identical']['instrument_digest'], builds['candidate']['instrument_digest'])
        self.assertNotEqual(builds['baseline-identical']['sdk_digest'], builds['candidate']['sdk_digest'])

    def test_the_build_saves_the_merge_base_fixture_set(self):
        # The merge base's generator runs against its own tree: a sibling
        # import resolves there, and it leaves no bytecode behind.
        (self.base/'expectations.py').write_text("EXPECTED = {'status': 'fails'}\n")
        (self.base/'fixtures.py').write_text('from expectations import EXPECTED\n'
            "def fixtures():\n    return {'w': {'value': '1', 'expected': EXPECTED, 'modes': ['eval']}}\n")
        metadata, _ = self.build(identical_builds=True)
        saved = self.root/'output'/compare.BASELINE_FIXTURES
        self.assertEqual(json.loads(saved.read_text(encoding='utf-8')),
                         {'w': {'value': '1', 'expected': {'status': 'fails'}, 'modes': ['eval']}})
        self.assertEqual(metadata['baseline_fixtures_sha256'], compare.digest(saved))
        self.assertFalse((self.root/'sources/baseline/tools/evaluator-bench/__pycache__').exists())

    def test_a_merge_base_without_fixtures_leaves_no_saved_set(self):
        (self.root/'output').mkdir()
        (self.root/'output'/compare.BASELINE_FIXTURES).write_text('{"stale": {}}')
        metadata, _ = self.build(identical_builds=True)
        self.assertIsNone(metadata['baseline_fixtures_sha256'])
        self.assertFalse((self.root/'output'/compare.BASELINE_FIXTURES).exists())

    def test_a_failing_merge_base_fixture_generator_fails_the_build(self):
        (self.base/'fixtures.py').write_text("def fixtures():\n    raise ValueError('broken generator')\n")
        with self.assertRaisesRegex(RuntimeError, '(?s)did not produce fixtures.*broken generator'):
            self.build(identical_builds=True)
        self.assertFalse((self.root/'target').exists())

    def test_reused_target_directories_are_refused_before_building(self):
        (self.root/'target/baseline-identical').mkdir(parents=True)
        with self.assertRaisesRegex(RuntimeError, 'reuse'):
            self.build(identical_builds=True)
        self.assertFalse((self.root/'bin').exists())

    def test_baseline_built_from_candidate_sources_is_refused(self):
        def candidate_sdk(name, sources):
            if name == 'baseline-identical':
                return dict(instrument=compare.tree_digest(sources[1]/'tools/evaluator-bench', compare.INSTRUMENT_SOURCES),
                            sdk=compare.tree_digest(sources[1], compare.SDK_SOURCES))
        with self.assertRaisesRegex(RuntimeError, 'baseline-identical binary was not built from.*SDK sources are those of'):
            self.build(identical_builds=True, reports=candidate_sdk)

    def test_fallback_binary_reused_from_the_identical_attempt_is_refused(self):
        # The merge base's own instrument shares the merge base's SDK path with
        # the identical copy; a reused binary still reports the copy's sources.
        def identical_copy(name, sources):
            if name == 'baseline':
                return dict(instrument=compare.tree_digest(sources[1]/'tools/evaluator-bench', compare.INSTRUMENT_SOURCES),
                            sdk=compare.tree_digest(sources[0], compare.SDK_SOURCES))
        with self.assertRaisesRegex(RuntimeError, 'baseline binary was not built from'):
            self.build(reports=identical_copy)

    def test_source_digests_match_the_build_script(self):
        # Known vector from build.rs: nested files, UTF-8 contents, an empty
        # file, a skipped symbolic link and skipped `target` entries.
        tree = self.root/'tree'
        for name, data in [('src/main.rs', b'fn main() {}\n'), ('src/nested/data.txt', 'café\n'.encode()),
                           ('src/target/x', b'ignored'), ('Cargo.toml', b'[package]\n'),
                           ('crates/a/target/out', b'built'), ('crates/a/lib.rs', b'pub fn a() {}\n'),
                           ('vendor/b/empty', b'')]:
            (tree/name).parent.mkdir(parents=True, exist_ok=True)
            (tree/name).write_bytes(data)
        (tree/'src/link.toml').symlink_to('../Cargo.toml')
        self.assertEqual(compare.tree_digest(tree, compare.INSTRUMENT_SOURCES), 'f557db8b34debee6')
        self.assertEqual(compare.tree_digest(tree, compare.SDK_SOURCES), '38845d3fe1785c2a')

    def test_confined_fallback_builds_each_own_instrument_and_records_diff(self):
        metadata, calls = self.build()
        self.assertEqual(metadata['kind'], 'cross-API')
        self.assertEqual([directory.name for _, directory in calls],
                         ['candidate', 'candidate-alloc', 'baseline-identical', 'baseline', 'baseline-alloc'])
        self.assertEqual(metadata['builds']['baseline']['instrument'],
                         str((self.root/'sources/baseline/tools/evaluator-bench').resolve()))
        self.assertEqual(json.loads((self.root/'bin/baseline').read_text(encoding='utf-8'))['instrument'],
                         compare.tree_digest(self.root/'sources/baseline/tools/evaluator-bench', compare.INSTRUMENT_SOURCES))
        self.assertTrue((self.root/'output/instrument.diff').read_text(encoding='utf-8'))
        self.assertEqual(json.loads((self.root/'output/instrument-pair.json').read_text(encoding='utf-8')), metadata)
        for name in ['baseline', 'candidate', 'baseline-alloc', 'candidate-alloc']:
            self.assertTrue((self.root/'bin'/name).exists())

    def test_a_fallback_may_declare_outcome_changes_but_not_change_requests(self):
        row = {'value': '1', 'modes': ['eval'], 'expected': {'status': 'fails'}}
        changed = dict(row, expected={'status': 'no-verdict'})
        metadata, _ = self.build(fixtures=[{'w': row}, {'w': changed}])
        self.assertEqual(metadata['kind'], 'cross-API')
        self.assertEqual(metadata['instrument_diff']['changed'], ['fixtures.py', 'src/sdk.rs'])
        shutil.rmtree(self.root/'sources')
        shutil.rmtree(self.root/'target')
        metadata, _ = self.build(fixtures=[{'w': row}, {'w': dict(changed, value='2')}])
        self.assertEqual(metadata['kind'], 'not comparable')
        self.assertIn('fixture expectations', metadata['reason'])

    def test_unconfined_fallback_stops_as_not_comparable(self):
        metadata, calls = self.build(confined=False)
        self.assertEqual(metadata['kind'], 'not comparable')
        self.assertEqual(len(calls), 3)
        self.assertFalse((self.root/'bin/baseline').exists())

    def test_candidate_and_own_baseline_build_failures_are_errors(self):
        with self.assertRaisesRegex(RuntimeError, 'Candidate'):
            self.build(fail_candidate=True)
        # A second source copy needs a fresh temporary tree.
        shutil.rmtree(self.root/'sources')
        with self.assertRaisesRegex(RuntimeError, 'baseline'):
            self.build(fail_own_base=True)


class FixtureCoverage(unittest.TestCase):
    def test_existing_fixture_requests_remain_identical(self):
        import hashlib
        # Preserve the preexisting replay requests, including exact number
        # tokens; an expectation may change to declare an outcome change.
        existing = {name: row for name, row in compare.requests(fixtures()).items()
                    if not name.startswith(('document-', 'read-', 'explain-', 'unevaluated-'))}
        self.assertEqual(len(existing), 32)
        serialized = (json.dumps(existing, indent=2)+'\n').encode()
        self.assertEqual(hashlib.sha256(serialized).hexdigest(),
                         '22bfb211564d6b0a113d41ebc067f17aad42418ed6ed15867c422b1e5ebc54f2')

    def test_document_scales_shared_definitions_and_complete_reads(self):
        rows = fixtures()
        for count in [1, 50, 1000]:
            row = rows[f'document-{count}']
            document = json.loads(row['document'])
            self.assertEqual(len(document['operations']), count)
            self.assertEqual(rows[f'read-{count}']['document'], row['document'])
            self.assertEqual(rows[f'read-{count}']['expected']['reads']['operations'], count)
        large = json.loads(rows['document-1000']['document'])
        self.assertIn('$defs', large['schemas']['shared'])
        self.assertEqual(large['operations']['op0000']['input']['$ref'], '#/schemas/shared/$defs/item')
        self.assertEqual(rows['document-invalid']['expected']['finding_count'], 6)

    def test_unevaluated_walks_reach_recursive_patterns_through_references(self):
        row = fixtures()['unevaluated-patterns']
        schema = json.loads(row['schema'])
        walks = [name for name in schema['$defs'] if name.startswith('R')]
        self.assertEqual(len(walks), 100)
        for name in walks:
            self.assertEqual(schema['$defs'][name], {'unevaluatedProperties': False, '$ref': '#/$defs/patterns'})
        patterns = schema['$defs']['patterns']['patternProperties']
        self.assertEqual(len(patterns), 5)
        for subschema in patterns.values():
            self.assertEqual(subschema['properties']['x'], {'$ref': '#'})
        self.assertIn('eval', row['modes'])
        self.assertEqual(row['expected'], {'status': 'satisfies'})

    def test_explanations_check_count_code_and_both_original_locations(self):
        rows = fixtures()
        self.assertGreaterEqual(len(json.loads(rows['explain-many']['value'])), 32)
        for name in ['explain-one', 'explain-many']:
            expected = rows[name]['expected']
            self.assertEqual(expected['problem_count'], 1)
            self.assertEqual(expected['first_problem']['code'], 'type')
            self.assertEqual(expected['first_problem']['instance_pointer'], '/0')
            self.assertEqual(expected['first_problem']['schema_location']['pointer'], '/items/type')

if __name__ == '__main__': unittest.main()
