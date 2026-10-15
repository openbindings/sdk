"""Keep implementation and review campaign labels out of maintained product files."""
from pathlib import Path
import argparse
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
# The two release narratives are maintained by the documentation workflow.
# Every other product file is checked, including new files and frozen inputs.
EXEMPT = {'CHANGELOG.md', 'docs/migration.md'}
PATTERNS = [
    re.compile(r'(?<![A-Za-z0-9])S(?:10|[1-9])(?:[a-eA-E]{1,2}(?:/[a-eA-E])?)?(?![A-Za-z0-9])'),
    re.compile(r'(?<![A-Za-z0-9])s(?:10|[1-9])[a-e]{0,2}[_-](?=[A-Za-z])'),
    re.compile(r'(?<![A-Za-z0-9])(?:CR|EV|API|HD|RO|N|PKG|OAC)-\d+[a-z]?\b'),
    re.compile(r'(?<![A-Za-z0-9])(?:ro|cr|ev|hd|api|oac|pkg|n)\d+_'),
    re.compile(r'(?<![A-Za-z0-9])s(?:10|[1-9])[a-e]{1,2}(?=[/._-])'),
    re.compile(r'\bD-[A-Z][A-Z0-9-]*\b'),
    re.compile(r'\b(?:slice|phase|unit)\s+[VX]\b|\bV[-_]map\b', re.I),
    re.compile(r'\b[\w-]+-loop-\d{4}-\d{2}-\d{2}\b|\b(?:role|role-contract)-loop\b'),
    re.compile(r'\bround[-_ ]0?[1-9]\b|\bloop[-_][A-Z]\d*\b', re.I),
]


def matches(text):
    return any(pattern.search(text) for pattern in PATTERNS)


def violations(name, content):
    if name in EXEMPT:
        return []
    result = [f'{name}: filename contains a campaign label'] if (
        matches(name) or re.search(r'(?:^|/)[vVxX](?:[_.-]|$)', name)
    ) else []
    for number, line in enumerate(content.splitlines(), 1):
        # Reconstruction patches retain upstream identifiers in removed/context
        # lines; only additions are maintained product content.
        if name.endswith('.patch') and (not line.startswith('+') or line.startswith('+++')):
            continue
        if matches(line):
            result.append(f'{name}:{number}: {line.strip()}')
    return result


def product_files():
    if (ROOT / '.git').exists():
        names = subprocess.check_output(
            ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=ROOT,
        ).decode().split('\0')
        return sorted({ROOT / name for name in names if name and (ROOT / name).is_file()})
    ignored = {'.git', 'target', 'node_modules', 'dist', '__pycache__'}
    return sorted(path for path in ROOT.rglob('*') if path.is_file()
                  and not ignored.intersection(path.relative_to(ROOT).parts))


def self_test():
    for label in [*('S' + str(i) for i in range(1, 11)), 'S' + '6e', 's' + '6e_resolver', 's' + '6de_review', 'S' + '6DE', 's' + '6c/fixture',
                  * (prefix + '-3' for prefix in ['CR', 'EV', 'API', 'HD', 'RO', 'N', 'PKG', 'OAC']),
                  'ro' + '18_test', 'cr' + '2_test', 'D' + '-METASCHEMA',
                  'slice ' + 'V', 'phase ' + 'X', 'V' + '-map',
                  'sdk-maturity' + '-loop-2026-10-10', 'round' + '-01']:
        assert violations('src/check.rs', '// ' + label), label
        assert violations('tests/' + label + '.rs', ''), label
    for allowed in ['OBI-01 through OBI-13', 'RFC 3986 §5.2', 'JSON Schema §9.4.2',
                    'ES2022', 'oneOf(U,V)', 'impl<V> Value<V>', 'X-Request-ID',
                    'infinite-loop-detection.json', 'round-trip', 'let s1 = "x";']:
        assert not violations('src/check.rs', allowed), allowed
    label = 'S' + '6e'
    assert violations('tools/qualification/inputs/spec/upstream.md', label)
    assert violations('tests/' + 'v' + '.rs', '')
    assert violations('tests/' + 'x' + '.rs', '')
    assert not violations('CHANGELOG.md', label)
    assert violations('docs/new.md', label)
    assert not violations('docs/upstream.patch', '-' + label)
    assert violations('docs/upstream.patch', '+' + label)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    if parser.parse_args().self_test:
        self_test()
    errors = []
    for path in product_files():
        try:
            content = path.read_text(encoding='utf-8')
        except UnicodeError:
            content = ''
        errors.extend(violations(path.relative_to(ROOT).as_posix(), content))
    if errors:
        raise SystemExit('\n'.join(errors))
    print('Product labels: filenames and content passed')


if __name__ == '__main__':
    main()
