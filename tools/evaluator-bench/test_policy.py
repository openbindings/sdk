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

    def run_comparison(self, *, enforce, metric=None, timing='within-threshold', cross_api=False):
        """Feed deterministic instrument observations through the real report/exit path."""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arguments = ['compare.py', '--output', str(root/'output')]
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
                report = json.loads((output/'comparison.json').read_text())
                self.assertEqual(len(report['errors']), 7)
                self.assertEqual(report['advisory'], not enforce)
                self.assertTrue(report['timing_advisory'])

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
              reports=None):
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
        if not confined:
            (sources[1]/'tools/evaluator-bench/src/main.rs').write_text('changed timing\n')
        calls = []
        target = self.root/'target'
        def run(command, **options):
            if command[1:] == ['--provenance']:
                return subprocess.CompletedProcess(command, 0, stdout=Path(command[0]).read_text(), stderr='')
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
            self.assertEqual(json.loads((self.root/'bin'/name.removesuffix('-identical')).read_text())['sdk'],
                             builds[name]['sdk_digest'])
        self.assertTrue(Path(builds['baseline-identical']['instrument']).name.startswith('evaluator-candidate-'))
        self.assertEqual(builds['baseline-identical']['instrument_digest'], builds['candidate']['instrument_digest'])
        self.assertNotEqual(builds['baseline-identical']['sdk_digest'], builds['candidate']['sdk_digest'])

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
        self.assertEqual(json.loads((self.root/'bin/baseline').read_text())['instrument'],
                         compare.tree_digest(self.root/'sources/baseline/tools/evaluator-bench', compare.INSTRUMENT_SOURCES))
        self.assertTrue((self.root/'output/instrument.diff').read_text())
        self.assertEqual(json.loads((self.root/'output/instrument-pair.json').read_text()), metadata)
        for name in ['baseline', 'candidate', 'baseline-alloc', 'candidate-alloc']:
            self.assertTrue((self.root/'bin'/name).exists())

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
    def test_existing_fixture_bytes_and_expectations_remain_identical(self):
        import hashlib
        # Preserve the preexisting replay requests, including exact number tokens.
        existing = {name: row for name, row in fixtures().items()
                    if not name.startswith(('document-', 'read-', 'explain-'))}
        self.assertEqual(len(existing), 32)
        serialized = (json.dumps(existing, indent=2)+'\n').encode()
        self.assertEqual(hashlib.sha256(serialized).hexdigest(),
                         '1a358b13151775452f266be52b2e775ff5b2673d1741c8816d1372609f13d8f1')

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
