"""Bind public applied-revision declarations to the SDK's pinned qualification inputs."""
import argparse
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'crates/openbindings-schema-evaluator-test-support/fixtures/alignment.json'
DECLARATIONS = [
    ('crates/openbindings/src/version.rs', 'APPLIED_SPEC_REVISION', 'spec_revision'),
    ('crates/openbindings-http-discovery/src/lib.rs', 'APPLIED_COMPANION_REVISION', 'spec_revision'),
    *[(f'crates/openbindings-{role}/src/lib.rs', 'APPLIED_INTERFACE_REVISION', 'interfaces_revision')
      for role in ['context', 'invocation', 'kind-description', 'source-pull', 'schema-comparison']],
]


def check(sources, pins):
    for path, name, authority in DECLARATIONS:
        expected = pins[authority]
        assert re.fullmatch('[0-9a-f]{40}', expected), f'invalid pinned revision: {authority}'
        declarations = re.findall(
            rf'^pub const {name}: &str = "([^"]*)";$', sources[path], re.MULTILINE)
        assert declarations == [expected], f'{path}: {name} does not identify the pinned {authority}'


def controls(sources, pins):
    rejected = 0
    for path, name, authority in DECLARATIONS:
        for mutation in ['stale', 'missing', 'duplicate']:
            changed = dict(sources)
            declaration = f'pub const {name}: &str = "{pins[authority]}";'
            replacement = {
                'stale': declaration.replace(pins[authority], '0' * 40),
                'missing': '',
                'duplicate': declaration + '\n' + declaration,
            }[mutation]
            changed[path] = changed[path].replace(declaration, replacement)
            try:
                check(changed, pins)
            except AssertionError:
                rejected += 1
            else:
                raise AssertionError(f'accepted {mutation} authority declaration: {path}')
    return rejected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    pins = json.loads(MANIFEST.read_text(encoding='utf-8'))
    sources = {path: (ROOT / path).read_text(encoding='utf-8') for path, _, _ in DECLARATIONS}
    check(sources, pins)
    print(json.dumps({'status': 'pass', 'declarations': len(DECLARATIONS),
                      'rejected_mutations': controls(sources, pins) if args.self_test else 0}))


if __name__ == '__main__':
    main()
