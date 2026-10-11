"""SDK/browser/build-free validation of preparation admission and safe extraction."""
import copy
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile
import subprocess
import sys
from common import HERE, bounded, check_seed, safe_extract, validate_request
from gate import activation, select
from activate import unzip_regular


def ready_request():
    request = json.loads((HERE / 'REQUEST.template.json').read_text())
    request['ownerLogin'] = 'synthetic-test-owner'
    request['candidate'] = {'commit': 'a' * 40, 'tree': 'b' * 40, 'expectedRustSourceSha256': 'c' * 64, 'expectedArchiveSha256': None}
    request['node']['archiveSha256'] = 'd' * 64
    request['playwright']['archiveSha256'] = 'e' * 64
    return request


def event(request, action='synchronize'):
    return {'sender': {'login': request['ownerLogin']}, 'action': action,
            'pull_request': {'state': 'open', 'head': {'repo': {'full_name': 'openbindings/sdk'}}}}


class PreparationTests(unittest.TestCase):
    def test_pending_candidate_or_download_digest_refuses(self):
        with self.assertRaises((ValueError, TypeError)): validate_request(json.loads((HERE / 'REQUEST.template.json').read_text()))
        for field in ('commit', 'tree', 'expectedRustSourceSha256'):
            request = ready_request(); request['candidate'][field] = None
            with self.assertRaises(ValueError): validate_request(request)
        request = ready_request(); request['node']['archiveSha256'] = None
        with self.assertRaises(ValueError): validate_request(request)

    def test_exact_baseline_and_repository_remain_frozen(self):
        request = ready_request(); validate_request(request)
        request['baseline']['tree'] = 'a' * 40
        with self.assertRaises(ValueError): validate_request(request)
        request = ready_request(); request['sdkRepository'] = 'someone/else'
        with self.assertRaises(ValueError): validate_request(request)

    def test_same_repo_explicit_actor_and_no_rerun_gate(self):
        request = ready_request(); valid = event(request)
        self.assertEqual(select(request, valid, 'openbindings/sdk', '1'), 'prepare')
        for changed in ('actor', 'fork', 'closed'):
            bad = copy.deepcopy(valid)
            if changed == 'actor': bad['sender']['login'] = 'different'
            if changed == 'fork': bad['pull_request']['head']['repo']['full_name'] = 'fork/sdk'
            if changed == 'closed': bad['pull_request']['state'] = 'closed'
            with self.assertRaises(ValueError): select(request, bad, 'openbindings/sdk', '1')
        with self.assertRaises(ValueError): select(request, valid, 'openbindings/sdk', '2')

    def test_campaign_requires_explicit_label_and_bound_archives(self):
        request = json.loads((HERE / 'ACTIVATION.template.json').read_text())
        with self.assertRaises(ValueError): activation(request)
        request.update(ownerLogin='synthetic-test-owner', activationId='a' * 32, preparationRunId=12, preparationRunAttempt=1,
                       artifactId=34, artifactName='browser-memory-inputs-12-1', candidateCommit='b' * 40, baselineCommit='c' * 40)
        for key in ('bundleSha256', 'bindingsSha256', 'ownerReviewSha256', 'candidateArchiveSha256', 'baselineArchiveSha256', 'preparerFreezeSha256'): request[key] = 'd' * 64
        self.assertEqual(select(request, event(request), 'openbindings/sdk', '1'), 'none')
        labeled = event(request, 'labeled'); labeled['label'] = {'name': 'sdk-browser-memory-campaign-approved'}
        self.assertEqual(select(request, labeled, 'openbindings/sdk', '1'), 'campaign')
        labeled['label']['name'] = 'random'
        with self.assertRaises(ValueError): select(request, labeled, 'openbindings/sdk', '1')

    def test_tar_traversal_duplicate_and_escaping_links_refused(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            root = Path(temporary)
            for index, names in enumerate([['../escape'], ['/absolute'], ['package/a', 'package/a']]):
                archive = root / f'{index}.tar'
                with tarfile.open(archive, 'w') as tar:
                    for name in names:
                        member = tarfile.TarInfo(name); member.size = 1; tar.addfile(member, io.BytesIO(b'x'))
                destination = root / f'extract{index}'
                with self.assertRaises(ValueError): safe_extract(archive, destination)
                self.assertFalse(destination.exists())
            archive = root / 'link.tar'
            with tarfile.open(archive, 'w') as tar:
                member = tarfile.TarInfo('package/link'); member.type = tarfile.SYMTYPE; member.linkname = '../../outside'; tar.addfile(member)
            with self.assertRaises(ValueError): safe_extract(archive, root / 'link', expected_top='package')

    def test_safe_node_style_relative_symlink_and_empty_destination(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            root = Path(temporary); archive = root / 'safe.tar'
            with tarfile.open(archive, 'w') as tar:
                member = tarfile.TarInfo('node/lib/tool'); member.size = 1; tar.addfile(member, io.BytesIO(b'x'))
                member = tarfile.TarInfo('node/bin/tool'); member.type = tarfile.SYMTYPE; member.linkname = '../lib/tool'; tar.addfile(member)
            safe_extract(archive, root / 'extracted', expected_top='node')
            self.assertEqual((root / 'extracted/node/bin/tool').read_bytes(), b'x')
            with self.assertRaises(ValueError): safe_extract(archive, root / 'extracted')

    def test_zip_traversal_refused_before_extraction(self):
        with tempfile.TemporaryDirectory(dir=HERE) as temporary:
            root = Path(temporary); archive = root / 'bad.zip'
            with zipfile.ZipFile(archive, 'w') as zipped: zipped.writestr('../escape', 'bad')
            with self.assertRaises(ValueError): unzip_regular(archive, root / 'extract')
            self.assertFalse((root / 'extract').exists())

    def test_reviewed_harness_and_synthetic_fixture_seed(self):
        freeze = check_seed(HERE / 'seed')
        self.assertEqual(freeze['status'], 'source-frozen-independent-review-and-host-controls-pending')

    def test_timeout_targets_only_its_created_process_group(self):
        with self.assertRaises(subprocess.TimeoutExpired):
            bounded([sys.executable, '-B', '-c', 'import time; time.sleep(60)'], timeout=0.1,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        result = bounded([sys.executable, '-B', '-c', 'pass'], timeout=10)
        self.assertEqual(result.returncode, 0)


if __name__ == '__main__': unittest.main(verbosity=2)
