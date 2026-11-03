"""Regression for a failed rerun after a successful qualification at the same HEAD."""
from pathlib import Path
import json
import io
import os
import runpy
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

HELPER = runpy.run_path(str(Path(__file__).with_name('verify-source-pull.py')))


class ReceiptTests(unittest.TestCase):
    helper = HELPER
    interface = "source-pull"
    def test_reextracted_package_sources_do_not_keep_normalized_old_timestamps(self):
        with tempfile.TemporaryDirectory(prefix='interface-archive-') as temporary:
            root = Path(temporary)
            archive = root / 'provider.crate'
            destination = root / 'extracted'
            source = destination / 'provider/src/lib.rs'
            for body in [b'old API', b'new API']:
                with tarfile.open(archive, 'w:gz') as packed:
                    member = tarfile.TarInfo('provider/src/lib.rs')
                    member.mtime = 1
                    member.size = len(body)
                    packed.addfile(member, io.BytesIO(body))
                self.helper['extract_fresh'](archive, destination)
                self.assertEqual(source.read_bytes(), body)
                self.assertGreater(source.stat().st_mtime, 1)

    def test_first_command_failure_invalidates_previous_success(self):
        with tempfile.TemporaryDirectory(prefix='interface-receipt-') as temporary:
            out = Path(temporary) / f'{self.interface}-qualification/source'
            out.mkdir(parents=True)
            (out / 'receipt.json').write_text('{"head":"same-head","commands":[]}')
            failure = subprocess.CompletedProcess(['cargo', 'test'], 1, '', 'injected failure')
            with patch.dict(os.environ, {'CARGO_TARGET_DIR': temporary}), \
                 patch.object(sys, 'argv', [f'verify-{self.interface}.py']), \
                 patch('subprocess.run', return_value=failure):
                with self.assertRaises(SystemExit):
                    self.helper['main']()
            self.assertFalse((out / 'receipt.json').exists())
            commands = json.loads((out / 'commands.json').read_text())
            self.assertEqual(len(commands), 1)
            self.assertEqual(commands[0]['exit'], 1)
            self.assertIn('injected failure', (out / 'consumer-test.log').read_text())


class InvocationReceiptTests(ReceiptTests):
    helper = runpy.run_path(str(Path(__file__).with_name('verify-invocation.py')))
    interface = "invocation"


class ResolutionTests(unittest.TestCase):
    def test_both_consumers_require_pinned_identities_and_extracted_paths(self):
        helper = runpy.run_path(str(Path(__file__).with_name('verify-invocation.py')))
        verify = helper['verify_resolution']
        with tempfile.TemporaryDirectory(prefix='interface-resolution-') as temporary:
            root = Path(temporary)
            out = root / 'target/archives'
            source = out / 'cargo/openbindings-0.2.0-alpha.1'
            baseline = '''version = 4
[[package]]
name = "openbindings"
version = "0.2.0-alpha.1"
[[package]]
name = "registry-dependency"
version = "1.0.0"
source = "registry+https://example.invalid/index"
checksum = "pinned"
'''
            (root / 'Cargo.lock').write_text(baseline)
            with patch.dict(verify.__globals__, {'ROOT': root}):
                for fixture in ['invocation-consumer', 'invoker-only-consumer']:
                    consumer = out / fixture
                    consumer.mkdir(parents=True)
                    lock = consumer / 'Cargo.lock'
                    original = baseline + f'[[package]]\nname = "{fixture}"\nversion = "0.0.0"\n'
                    lock.write_text(original)
                    package = {'name': 'openbindings', 'version': '0.2.0-alpha.1',
                               'source': None, 'manifest_path': str(source / 'Cargo.toml')}
                    metadata = {'packages': [package]}
                    arguments = [metadata, consumer, {'openbindings': source},
                                 {fixture: consumer}, True, out]
                    self.assertEqual(len(verify(*arguments)), 64)
                    for changed in [original.replace('1.0.0', '9.0.0'),
                                    original.replace('checksum = "pinned"', 'checksum = "changed"'),
                                    original + '[[package]]\nname = "unknown-fixture"\nversion = "0.0.0"\n']:
                        lock.write_text(changed)
                        with self.assertRaisesRegex(SystemExit, 'changed pinned'):
                            verify(*arguments)
                    lock.write_text(original)
                    package['manifest_path'] = str(root / 'crates/openbindings/Cargo.toml')
                    with self.assertRaisesRegex(SystemExit, 'unexpected local'):
                        verify(*arguments)
                    package['manifest_path'] = str(source / 'Cargo.toml')
                    package['version'] = '9.0.0'
                    with self.assertRaisesRegex(SystemExit, 'disagrees'):
                        verify(*arguments)


if __name__ == '__main__':
    unittest.main()
