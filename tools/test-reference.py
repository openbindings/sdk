"""Regression checks for source documentation and generated reference links."""
import importlib.util
from pathlib import Path
import tempfile
import unittest


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReferenceLinks(unittest.TestCase):
    def test_rustdoc_local_targets_fragments_and_fences(self):
        links = load('verify-doc-links')
        with tempfile.TemporaryDirectory() as directory:
            links.ROOT = root = Path(directory)
            (root / 'guide.md').write_text('# Resources, diagnostics and lifetime\n')
            source = root / 'api.rs'
            source.write_text('''// [ordinary comment](missing.md)
/// [guide](guide.md#resources-diagnostics-and-lifetime)
/// ```rust
/// [code](missing.md)
/// ```
//! [broken](guide.md#retained-work-and-replacement)
/** [block](missing.md) */
/*! Inner documentation.
 * [reference]: guide.md#resources-diagnostics-and-lifetime
 */
''')
            errors, checked, _ = links.check([source])
            self.assertEqual(checked, 4)
            self.assertEqual(len(errors), 2)
            self.assertIn('api.rs:6: missing heading', errors[0])
            self.assertIn('api.rs:7: missing target', errors[1])

    def test_inherited_links_are_versioned_and_unknown_links_still_fail(self):
        reference = load('build-reference')
        with tempfile.TemporaryDirectory() as directory:
            reference.ROOT = root = Path(directory)
            (root / 'Cargo.lock').write_text('''[[package]]
name = "tracing"
version = "0.1.44"
[[package]]
name = "futures-core"
version = "0.3.32"
''')
            output = root / 'bundle'
            page = output / 'rust/openbindings_http_discovery/struct.Cancellation.html'
            page.parent.mkdir(parents=True)
            page.write_text('<a href="futures_core::future::TryFuture">TryFuture</a>' * 2)
            repairs = reference.repair_inherited_links(output)
            self.assertEqual(len(repairs), 1)
            self.assertEqual(repairs[0]['count'], 2)
            self.assertEqual(repairs[0]['to'],
                             'https://docs.rs/futures-core/0.3.32/futures_core/future/trait.TryFuture.html')
            reference.verify_local_links(output)
            page.write_text(page.read_text() + '<a href="unknown.html">unknown</a>')
            reference.repair_inherited_links(output)
            with self.assertRaisesRegex(SystemExit, 'missing local target unknown.html'):
                reference.verify_local_links(output)


if __name__ == '__main__':
    unittest.main()
