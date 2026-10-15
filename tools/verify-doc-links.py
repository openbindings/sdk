"""Check maintained documentation links and fragments without network access."""
from pathlib import Path
from urllib.parse import unquote, urlsplit
import re

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'https://github.com/openbindings/sdk/'


def prose(text):
    """Remove fenced code while preserving line numbers for diagnostics."""
    output, fence = [], None
    for line in text.splitlines():
        marker = re.match(r'^\s*(`{3,}|~{3,})(.*)$', line)
        if fence:
            if marker and marker[1][0] == fence[0] and len(marker[1]) >= len(fence) and not marker[2].strip():
                fence = None
            output.append('')
        elif marker:
            fence = marker[1]
            output.append('')
        else:
            output.append(line)
    return '\n'.join(output)


def anchors(path):
    text = prose(path.read_text())
    found = set(re.findall(r'(?:id|name)=["\']([^"\']+)', text))
    counts = {}
    for line in text.splitlines():
        match = re.match(r'^ {0,3}#{1,6}\s+(.+?)\s*#*$', line)
        if not match:
            continue
        heading = re.sub(r'<[^>]+>', '', match[1]).lower()
        heading = re.sub(r'[^\w\-\s]', '', heading)
        slug = re.sub(r'\s', '-', heading)
        index = counts.get(slug, 0)
        counts[slug] = index + 1
        found.add(slug if not index else f'{slug}-{index}')
    return found


def main():
    paths = sorted(set([
        *ROOT.glob('*.md'), *ROOT.glob('docs/*.md'),
        *ROOT.glob('crates/*/README.md'),
        ROOT / 'packages/typescript/README.md',
    ]))
    errors, checked, remote = [], 0, set()
    for path in paths:
        text = prose(path.read_text())
        # Inline links, reference definitions and HTML links. Code spans alone
        # are not links; no HTTP requests are made for external destinations.
        pattern = r'\]\(([^\s)]+)(?:\s+["\'][^\n]*?["\'])?\)|^ {0,3}\[[^\]]+\]:\s*(\S+)|(?:href|src)=["\']([^"\']+)'
        for match in re.finditer(pattern, text, re.M):
            raw = next(group for group in match.groups() if group is not None).strip('<>')
            parsed = urlsplit(raw)
            local = None
            if raw.startswith(REPOSITORY):
                relative = parsed.path.removeprefix('/openbindings/sdk/')
                for prefix in ('blob/main/', 'tree/main/'):
                    if relative.startswith(prefix):
                        local = ROOT / unquote(relative.removeprefix(prefix))
                        break
            elif not parsed.scheme and not parsed.netloc:
                local = path.parent / unquote(parsed.path) if parsed.path else path
            if local is None:
                remote.add(raw)
                continue
            checked += 1
            location = f'{path.relative_to(ROOT)}:{text.count(chr(10), 0, match.start()) + 1}'
            if not local.exists():
                errors.append(f'{location}: missing target {raw}')
            elif parsed.fragment and local.suffix == '.md' and unquote(parsed.fragment) not in anchors(local):
                errors.append(f'{location}: missing heading {raw}')
    if errors:
        raise SystemExit('\n'.join(errors))
    print(f'Documentation links: {checked} local targets/fragments in {len(paths)} pages passed; '
          f'{len(remote)} external URLs unchecked (offline).')


if __name__ == '__main__':
    main()
