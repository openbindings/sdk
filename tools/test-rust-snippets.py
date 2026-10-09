"""Focused fence-selection controls for the two first-use Markdown documents."""
from pathlib import Path
import runpy
import tempfile
import unittest

HELPER = runpy.run_path(str(Path(__file__).with_name('verify-rust-snippets.py')))
SOURCE = b'fn main() {}\n'
RUST = b'```rust\n' + SOURCE + b'```\n'


class RustFenceTests(unittest.TestCase):
    def read(self, markdown):
        with tempfile.TemporaryDirectory(prefix='sdk-rust-fence-test-') as temporary:
            path = Path(temporary) / 'example.md'
            path.write_bytes(markdown)
            return HELPER['read_snippet'](path)

    def test_preserves_verbatim_bytes(self):
        cases = {
            'lf': (RUST, SOURCE),
            'crlf': (RUST.replace(b'\n', b'\r\n'), SOURCE.replace(b'\n', b'\r\n')),
            'hidden-line': (b'```rust\n# Ok(())\n```\n', b'# Ok(())\n'),
            'tilde': (b'~~~~rust\n' + SOURCE + b'~~~~\n', SOURCE),
            'longer-close': (b'```rust\n' + SOURCE + b'`````\n', SOURCE),
            'close-whitespace': (b'```rust\n' + SOURCE + b'   ``` \t\n', SOURCE),
            'no-final-newline': (RUST[:-1], SOURCE),
            'short-fence-is-content': (b'````rust\n```\n````\n', b'```\n'),
            'other-delimiter-is-content': (b'```rust\n~~~\n```\n', b'~~~\n'),
            'close-suffix-is-content': (b'```rust\n```text\n```\n', b'```text\n'),
            'rust-inside-shell-is-content': (b'```sh\n```rust\nignored\n```\n' + RUST, SOURCE),
            'longer-shell-fence': (b'````sh\n```rust\nignored\n```\n````\n' + RUST, SOURCE),
        }
        for name, (markdown, expected) in cases.items():
            with self.subTest(name=name):
                self.assertEqual(self.read(markdown), expected)

    def test_rejects_missing_extra_or_unclosed_fences(self):
        cases = {
            'missing': b'No Rust example.\n',
            'extra': RUST + RUST,
            'unclosed-rust': b'```rust\n' + SOURCE,
            'unclosed-extra': RUST + b'```rust\n',
            'unclosed-other-language': RUST + b'```sh\n',
            'short-close': b'````rust\n' + SOURCE + b'```\n',
            'wrong-close-character': b'```rust\n' + SOURCE + b'~~~\n',
            'close-with-suffix': b'```rust\n' + SOURCE + b'```oops\n',
            'malformed-opener': b'```rust`\n' + SOURCE + b'```\n',
            'swallowed': b'```sh\ncommand\n' + RUST,
        }
        for name, markdown in cases.items():
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    self.read(markdown)

    def test_guide_rejects_deleted_shell_close(self):
        guide = (HELPER['ROOT'] / 'docs/rust-first-use.md').read_bytes()
        lines = guide.splitlines(keepends=True)
        command = b'cargo run -p openbindings-json-schema-evaluator --example replacement'
        matches = [i for i, line in enumerate(lines) if line.rstrip(b'\r\n') == command]
        self.assertEqual(len(matches), 1)
        close = matches[0] + 1
        self.assertEqual(lines[close].rstrip(b'\r\n'), b'```')
        with self.assertRaises(ValueError):
            self.read(b''.join(lines[:close] + lines[close + 1:]))


if __name__ == '__main__':
    unittest.main()
