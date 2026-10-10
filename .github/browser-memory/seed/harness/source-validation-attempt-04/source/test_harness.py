"""SDK-free tests with explicitly synthetic counters. Not Linux-control evidence."""
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import campaign
import identity
import kernel

HERE = Path(__file__).resolve().parent


def raw_snapshot(current=100, peak=200):
    return {'memory.current': str(current), 'memory.peak': str(peak),
            'memory.stat': 'anon 80\nfile 10\nkernel 10\npgfault 20\n',
            'memory.events': 'low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n',
            'memory.swap.current': '0', 'memory.swap.peak': '0',
            'memory.swap.events': 'high 0\nmax 0\nfail 0\n',
            'memory.max': str(kernel.CEILING), 'memory.swap.max': '0',
            'cgroup.events': 'populated 0\nfrozen 0\n', 'cgroup.procs': ''}


def row(phase, current=100, peak=200, owners=2):
    raw = raw_snapshot(current, peak)
    return {'phase': phase, 'raw': raw, 'parsed': kernel.parse_snapshot(raw),
            'worker': {'storageOwners': owners, 'wasmCapacityBytes': 1024 * 1024}}


def result(mode='ordinary'):
    return {'status': 'passed', 'worker': {'status': 'passed', 'timed': False, 'forcedGc': False,
            'sdkLoaded': mode != 'control', 'fixedWarmedOwners': 2, 'retainedCalls': 1000, 'replacements': 400}}


class HarnessTests(unittest.TestCase):
    def test_snapshot_requires_every_counter(self):
        for key in kernel.COUNTERS:
            raw = raw_snapshot(); del raw[key]
            with self.assertRaises(ValueError, msg=key): kernel.parse_snapshot(raw)

    def test_malformed_integer_and_duplicate_key_refused(self):
        for value in ('-1', 'max', '1.0', '', 'NaN', '1\n2', ' 2'):
            with self.assertRaises(ValueError): kernel.uint(value)
        with self.assertRaises(ValueError): kernel.pairs('oom 0\noom 0\n', ('oom',))
        raw = raw_snapshot(200, 100)
        with self.assertRaises(ValueError): kernel.parse_snapshot(raw)

    def test_safety_limits_and_swap_fail_closed(self):
        for key, value in [('memory.max', '999'), ('memory.swap.max', '1'), ('cgroup.events', 'populated 0\nfrozen 1\n')]:
            raw = raw_snapshot(); raw[key] = value
            with self.assertRaises(ValueError): kernel.parse_snapshot(raw)
        for key in ('memory.swap.current', 'memory.swap.peak'):
            raw = raw_snapshot(); raw[key] = '1'
            with self.assertRaises(ValueError): kernel.healthy([{'parsed': kernel.parse_snapshot(raw)}])
        for key in ('max', 'oom', 'oom_kill'):
            raw = raw_snapshot(); raw['memory.events'] = raw['memory.events'].replace(key + ' 0', key + ' 1')
            with self.assertRaises(ValueError): kernel.healthy([{'parsed': kernel.parse_snapshot(raw)}])

    def test_fixed_schedule(self):
        jobs = kernel.schedule()
        self.assertEqual(len(jobs), 42)
        self.assertEqual(len({j['id'] for j in jobs}), 42)
        for browser in ('chromium', 'webkit'):
            for artifact in ('baseline', 'candidate'):
                for profile in ('small', 'catalog', 'large'):
                    self.assertEqual([j['trial'] for j in jobs if j['browser'] == browser and j['artifact'] == artifact and j.get('profile') == profile], [1, 2, 3])
            self.assertEqual(sum(j['browser'] == browser and j['mode'] == 'lifecycle' for j in jobs), 3)

    def test_peak_is_since_launch_not_reset_at_warm_idle(self):
        rows = [row('warm-released', 100 * kernel.MIB, 180 * kernel.MIB), row('all-released', 100 * kernel.MIB, 180 * kernel.MIB)]
        with self.assertRaisesRegex(ValueError, 'incremental'):
            kernel.judge({'mode': 'ordinary', 'profile': 'small'}, rows, result())
        decision = kernel.judge({'mode': 'ordinary', 'profile': 'catalog'}, rows, result())
        self.assertEqual(decision['conservativeIncrementalPeakBytes'], 80 * kernel.MIB)

    def test_lifecycle_drift_is_not_excused_by_exact_owner_return(self):
        rows = [row('warm-released'), row('1000-retained-calls', owners=4), row('released-200'),
                row('released-400', 9 * kernel.MIB, 10 * kernel.MIB), row('all-released')]
        self.assertEqual(kernel.judge({'mode': 'lifecycle'}, rows, result('lifecycle'))['status'], 'investigation-required')
        rows[2]['worker']['storageOwners'] += 1
        with self.assertRaisesRegex(ValueError, 'owner'):
            kernel.judge({'mode': 'lifecycle'}, rows, result('lifecycle'))

    def test_incomplete_out_of_order_or_gc_evidence_refused(self):
        rows = [row('warm-released'), row('all-released')]
        for bad in (rows[:1], rows[::-1], rows + [rows[-1]]):
            with self.assertRaises(ValueError): kernel.judge({'mode': 'ordinary', 'profile': 'small'}, bad, result())
        bad = result(); bad['worker']['forcedGc'] = True
        with self.assertRaises(ValueError): kernel.judge({'mode': 'ordinary', 'profile': 'small'}, rows, bad)

    def test_control_current_growth_and_dropped_references_are_separate(self):
        rows = [row('idle'), row('held-64MiB', 64 * kernel.MIB, 65 * kernel.MIB), row('dropped-references')]
        rows[1]['worker'].update(payloadBytes=64 * kernel.MIB, checksum=278528)
        rows[2]['worker']['payloadBytes'] = 0
        decision = kernel.judge({'mode': 'control'}, rows, result('control'))
        self.assertFalse(decision['physicalCollectionClaim'])
        rows[1]['parsed']['memory.current'] = 1024
        with self.assertRaises(ValueError): kernel.judge({'mode': 'control'}, rows, result('control'))

    def test_descendant_rejects_cycles_and_unrelated_pids(self):
        with patch.object(kernel, 'proc', side_effect=lambda pid: {'ppid': {5: 4, 4: 2, 7: 8, 8: 7}[pid]}):
            self.assertTrue(kernel.descendant(5, 2))
            self.assertFalse(kernel.descendant(7, 2))

    def test_package_archive_mismatch_is_admission_failure(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            root = Path(temporary); package = root / 'package'; package.mkdir()
            (package / 'dist/wasm').mkdir(parents=True)
            wasm = package / 'dist/wasm/openbindings_wasm_bg.wasm'; wasm.write_bytes(b'synthetic-test')
            archive = root / 'package.tgz'
            with tarfile.open(archive, 'w:gz') as tar: tar.add(package, arcname='package')
            source = {'sourceCommit': 'a' * 40, 'sourceTree': 'b' * 40, 'npmArchiveSha256': kernel.sha(archive)}
            receipt = root / 'source.json'; receipt.write_text(json.dumps(source))
            binding = dict(source, sourceReceipt={'path': str(receipt), 'sha256': kernel.sha(receipt)},
                           archive={'path': str(archive), 'sha256': kernel.sha(archive)}, packageRoot=str(package),
                           treeSha256=identity.tree(package)[0], wasmSha256=kernel.sha(wasm))
            identity.package(binding)
            wasm.write_bytes(b'different-but-tree-rebound')
            binding.update(treeSha256=identity.tree(package)[0], wasmSha256=kernel.sha(wasm))
            with self.assertRaisesRegex(ValueError, 'archive'): identity.package(binding)

    def test_tree_escape_and_source_tampering_refused(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            root = Path(temporary); (root / 'escape').symlink_to(HERE / 'kernel.py')
            with self.assertRaises(ValueError): identity.tree(root)
            (root / 'escape').unlink()
            names = ['campaign.py', 'kernel.py', 'identity.py', 'enter.py', 'driver.mjs', 'worker.mjs',
                     'touch.py', 'inspect-inputs.py', 'test_harness.py', 'PROFILES.json', 'replacement-fixture.json',
                     'BINDINGS.template.json', 'OWNER-REVIEW.template.json', 'HANDOFF.md', 'ubuntu-workflow.proposal.yml']
            for name in names: (root / name).write_text('synthetic source')
            source = root / 'kernel.py'
            (root / 'SOURCE-FREEZE.json').write_text(json.dumps({'schemaVersion': 1, 'files': {name: kernel.sha(root / name) for name in names}}))
            with patch.object(identity, 'HERE', root):
                identity.source_freeze(); source.write_text('changed')
                with self.assertRaises(ValueError): identity.source_freeze()

    def test_unsafe_cleanup_stops_before_any_write(self):
        group = kernel.Group.__new__(kernel.Group)
        with patch.object(group, 'guard', side_effect=ValueError('unsafe cleanup')):
            with self.assertRaisesRegex(ValueError, 'unsafe cleanup'): group.cleanup()
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            group.path = Path(temporary) / ('ob-memory-' + 'a' * 32)
            group.path.symlink_to(HERE, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'symlink'): group.guard()

    def test_sdk_free_payload_protocol_locally_without_kernel_claim(self):
        completed = subprocess.run([sys.executable, str(HERE / 'touch.py')], input='ack\nack\n',
                                   text=True, capture_output=True, check=True, timeout=15)
        messages = [json.loads(line) for line in completed.stdout.splitlines()]
        self.assertEqual([m.get('row', {}).get('phase') for m in messages[:2]], ['idle', 'held-64MiB'])
        self.assertEqual(messages[1]['row']['payloadBytes'], 64 * kernel.MIB)
        self.assertEqual(messages[2]['payloadBytes'], 0)
        self.assertEqual(messages[2]['status'], 'passed')


class ScratchRepairTests(unittest.TestCase):
    def make_scratch(self):
        scratch = campaign.Scratch(os.getuid(), os.getgid())
        scratch.create()
        self.addCleanup(scratch.close)
        return scratch

    def test_real_unix_sockets_fit_independently_of_long_evidence_path(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            output = Path(temporary) / ('long-evidence-' + 'x' * 160) / 'chromium-worker-control'
            output.mkdir(parents=True)
            evidence = output / 'RECEIPT.json'; evidence.write_text('preserve this evidence')
            old = output / 'driver/tmp/org.chromium.Chromium.XXXXXX/SingletonSocket'
            old.parent.mkdir(parents=True)
            with socket.socket(socket.AF_UNIX) as sock:
                with self.assertRaises(OSError): sock.bind(str(old))
            scratch = self.make_scratch()
            try:
                self.assertFalse(scratch.root.is_relative_to(output))
                self.assertEqual(stat.S_IMODE(scratch.root.stat().st_mode), 0o710)
                self.assertEqual(stat.S_IMODE(scratch.work.stat().st_mode), 0o700)
                self.assertEqual(scratch.work.stat().st_uid, os.getuid())
                for suffix in ('/org.chromium.Chromium.XXXXXX/SingletonSocket', campaign.Scratch.SOCKET_SUFFIX):
                    path = Path(str(scratch.tmp) + suffix)
                    path.parent.mkdir()
                    self.assertLessEqual(len(os.fsencode(path)), 100)
                    with socket.socket(socket.AF_UNIX) as sock: sock.bind(str(path))
                scratch.cleanup(True)
                self.assertTrue(scratch.info['removed'])
                self.assertFalse(scratch.root.exists())
                self.assertEqual(evidence.read_text(), 'preserve this evidence')
            finally:
                if scratch.root.exists(): scratch.cleanup(True)

    def test_cleanup_refuses_live_processes_and_never_follows_external_link(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            outside = Path(temporary); marker = outside / 'keep'; marker.write_text('unowned directory')
            scratch = self.make_scratch()
            try:
                (scratch.work / 'outside').symlink_to(outside, target_is_directory=True)
                with self.assertRaisesRegex(ValueError, 'processes may remain'): scratch.cleanup(False)
                self.assertTrue(scratch.root.exists())
                scratch.cleanup(True)
                self.assertEqual(marker.read_text(), 'unowned directory')
            finally:
                if scratch.root.exists(): scratch.cleanup(True)

    def test_cleanup_refuses_parent_identity_replacement_without_deleting_it(self):
        scratch = self.make_scratch()
        moved = scratch.root.with_name(scratch.root.name + '-saved')
        scratch.root.rename(moved)
        scratch.root.mkdir(mode=0o710)
        marker = scratch.root / 'keep'; marker.write_text('different directory')
        try:
            with self.assertRaisesRegex(ValueError, 'parent identity changed'): scratch.cleanup(True)
            self.assertEqual(marker.read_text(), 'different directory')
        finally:
            marker.unlink(); scratch.root.rmdir(); moved.rename(scratch.root)
            scratch.cleanup(True)

    def test_cleanup_refuses_permissions_and_work_symlink_replacement(self):
        scratch = self.make_scratch()
        try:
            scratch.root.chmod(0o777)
            with self.assertRaisesRegex(ValueError, 'owner/permissions'): scratch.cleanup(True)
            scratch.root.chmod(0o710)
            moved = scratch.root / 'original'; scratch.work.rename(moved)
            scratch.work.symlink_to(moved, target_is_directory=True)
            with self.assertRaises(ValueError): scratch.cleanup(True)
            self.assertTrue(moved.is_dir())
            scratch.work.unlink(); moved.rename(scratch.work)
            scratch.cleanup(True)
        finally:
            if scratch.root.exists(): scratch.cleanup(True)

    def failed_launch(self, output):
        group = SimpleNamespace(path=Path('/synthetic-cgroup-never-created'), identity=(0, 0), rows=[],
                                snapshot=lambda *args: {'parsed': {'cgroup.procs': [], 'cgroup.events': {'populated': 0}}},
                                cleanup=lambda: None)
        enrollment = SimpleNamespace(path=Path('/synthetic-socket-never-opened'), token='synthetic',
                                     receipt={'status': 'not-launched'}, close=lambda: None)
        binding = {'node': {'path': '/not-executed-node'},
                   'browsers': {'chromium': {'launcher': {'path': '/not-executed-browser'}}}}
        with patch.object(campaign, 'Group', return_value=group), patch.object(campaign, 'Enrollment', return_value=enrollment), \
                patch.object(campaign.subprocess, 'Popen', side_effect=OSError('synthetic launch failure')):
            result = campaign.run_job({'id': 'synthetic', 'mode': 'control', 'browser': 'chromium'}, binding, {},
                                      Path('/synthetic-parent-never-created'), output, os.getuid(), os.getgid())
        return result, json.loads((output / 'RECEIPT.json').read_text())

    def test_launch_failure_preserves_evidence_and_cleans_created_scratch(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            output = Path(temporary) / 'failed-job'
            result, receipt = self.failed_launch(output)
            self.assertEqual(result['status'], 'failed')
            self.assertTrue(result['safeToContinue'])
            self.assertTrue(receipt['scratch']['removed'])
            self.assertFalse(Path(receipt['scratch']['path']).exists())
            self.assertTrue((output / 'driver-input.json').exists())
            self.assertIn('synthetic launch failure', receipt['errors'][0])

    def test_cleanup_failure_stops_later_jobs_and_records_retained_scratch(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            # Retain the descriptor during this injected error so this unit test
            # can finish exact-identity cleanup after verifying the failed receipt.
            with patch.object(campaign.Scratch, 'cleanup', side_effect=OSError('synthetic cleanup failure')), \
                    patch.object(campaign.Scratch, 'close', autospec=True) as close:
                result, receipt = self.failed_launch(Path(temporary) / 'failed-cleanup')
            scratch = close.call_args.args[0]
            try:
                self.assertFalse(result['safeToContinue'])
                self.assertEqual(result['status'], 'failed')
                self.assertFalse(receipt['scratch']['removed'])
                self.assertIn('synthetic cleanup failure', receipt['scratch']['error'])
                self.assertTrue(scratch.root.exists())
            finally:
                scratch.cleanup(True); scratch.close()

    def test_stop_failure_status_is_separate_from_preparation_and_attempt(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            output = Path(temporary)
            preparation = {'status': 'passed', 'sdkAttemptConsumed': False}
            for phase in ('controls', 'pre-sdk-revalidation', 'parent-cleanup'):
                stopped = campaign.stop_receipt(preparation, output, ValueError('failed witness'), phase)
                campaign.save(output / 'STOP.json', stopped)
                actual = json.loads((output / 'STOP.json').read_text())
                self.assertEqual(actual['status'], 'failed')
                self.assertTrue(actual['preparationPassed'])
                self.assertFalse(actual['sdkAttemptConsumed'])
                self.assertEqual(actual['failurePhase'], phase)
            self.assertEqual(preparation['status'], 'passed')
            (output / 'ATTEMPT.json').write_text('{}')
            self.assertTrue(campaign.stop_receipt(preparation, output, ValueError('failure'), 'sdk-trials')['sdkAttemptConsumed'])
            self.assertFalse(campaign.stop_receipt({'status': 'failed'}, output, ValueError('failure'), 'admission')['preparationPassed'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
