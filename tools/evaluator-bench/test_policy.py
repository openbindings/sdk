"""Exercise threshold conjunctions, noise, confirmation and advisory policy inputs."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
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

if __name__ == '__main__': unittest.main()
