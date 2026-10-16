"""Exercise enforced outcomes/memory and advisory timing through the comparison driver."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
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

    def run_comparison(self, *, enforce, metric=None, timing='within-threshold'):
        """Feed deterministic instrument observations through the real report/exit path."""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arguments = ['compare.py', '--output', str(root/'output')]
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
                if binary.name.startswith('candidate') and metric in memory:
                    memory[metric] *= 2
                return dict(iterations=request['iterations'],
                            outcomes={'satisfies': request['iterations']},
                            observation={'status': 'satisfies'},
                            nanoseconds=request['iterations']*200_000_000,
                            memory=memory,
                            rss_bytes=8*1024*1024 if binary.name.startswith('candidate') else 1024*1024)

            with patch.object(sys, 'argv', arguments), \
                 patch.object(compare, 'fixtures', return_value={'scalar': {'modes': ['eval']}}), \
                 patch.object(compare, 'invoke', side_effect=invoke), \
                 patch.object(compare, 'time_decision', return_value=timing):
                try:
                    compare.main()
                    exit_code = 0
                except SystemExit as error:
                    exit_code = error.code
            return exit_code, json.loads((root/'output/comparison.json').read_text())

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
                report = json.loads((output/'comparison.json').read_text())
                self.assertEqual(len(report['errors']), 7)
                self.assertEqual(report['advisory'], not enforce)
                self.assertTrue(report['timing_advisory'])

if __name__ == '__main__': unittest.main()
