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

HELPER = runpy.run_path(str(Path(__file__).with_name('verify-source-inspection.py')))


class ReceiptTests(unittest.TestCase):
    helper = HELPER
    role = "source-inspection"
    def test_reextracted_package_sources_do_not_keep_normalized_old_timestamps(self):
        with tempfile.TemporaryDirectory(prefix='inspection-archive-') as temporary:
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
        with tempfile.TemporaryDirectory(prefix='inspection-receipt-') as temporary:
            out = Path(temporary) / f'{self.role}-qualification/source'
            out.mkdir(parents=True)
            (out / 'receipt.json').write_text('{"head":"same-head","commands":[]}')
            failure = subprocess.CompletedProcess(['cargo', 'test'], 1, '', 'injected failure')
            with patch.dict(os.environ, {'CARGO_TARGET_DIR': temporary}), \
                 patch.object(sys, 'argv', [f'verify-{self.role}.py']), \
                 patch('subprocess.run', return_value=failure):
                with self.assertRaises(SystemExit):
                    self.helper['main']()
            self.assertFalse((out / 'receipt.json').exists())
            commands = json.loads((out / 'commands.json').read_text())
            self.assertEqual(len(commands), 1)
            self.assertEqual(commands[0]['exit'], 1)
            self.assertIn('injected failure', (out / 'consumer-test.log').read_text())


class SynthesisReceiptTests(ReceiptTests):
    helper = runpy.run_path(str(Path(__file__).with_name('verify-interface-synthesis.py')))
    role = "interface-synthesis"


if __name__ == '__main__':
    unittest.main()
