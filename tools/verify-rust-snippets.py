"""Test repository prose, run standalone fences and verify the external walkthrough."""
from pathlib import Path
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SNIPPETS = [
    (ROOT / 'docs/rust-first-use.md', 'rust_first_use'),
    (ROOT / 'docs/document-to-typed-result.md', 'document_call'),
    (ROOT / 'crates/openbindings-json-schema-evaluator/README.md', 'evaluator_readme'),
]
CRATES = ['openbindings', 'openbindings_json_schema_evaluator', 'openbindings_invocation', 'serde', 'serde_json', 'tokio']


def read_snippet(path):
    # These documents use top-level fences. Track every language's fence so
    # a Rust-looking line inside another block cannot become an executable example.
    snippets = []
    fence = None
    body = []
    for number, line in enumerate(path.read_bytes().splitlines(keepends=True), 1):
        marker = re.fullmatch(rb' {0,3}(`{3,}|~{3,})([^\r\n]*)', line.rstrip(b'\r\n'))
        if fence is None:
            if marker is None:
                continue
            delimiter, info = marker.groups()
            if delimiter[:1] == b'`' and b'`' in info:
                raise ValueError(f'{path}:{number}: malformed backtick fence')
            fence = (delimiter, info.strip(b' \t') == b'rust', number)
            body = []
        else:
            delimiter, rust, opening = fence
            if (marker is not None and marker[1][:1] == delimiter[:1]
                    and len(marker[1]) >= len(delimiter) and not marker[2].strip(b' \t')):
                if rust:
                    snippets.append(b''.join(body))
                fence = None
            else:
                body.append(line)
    if fence is not None:
        raise ValueError(f'{path}:{fence[2]}: unclosed fence')
    if len(snippets) != 1:
        raise ValueError(f'{path}: expected exactly one complete Rust fence, found {len(snippets)}')
    return snippets[0]


def same_walkthrough(main_rs, fence):
    # A Windows checkout may give either file CRLF line endings. The walkthrough
    # must be the same program as the example crate, whatever its line endings.
    def lf(text):
        return text.replace(b'\r\n', b'\n')
    return lf(main_rs) == lf(fence) + b'\n#[cfg(test)]\nmod tests;\n'


def run(args, **kwargs):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), cwd=ROOT, check=True, **kwargs)


def main():
    # Require every documented example before building; a deleted fence is a failure.
    sources = [(path, name, read_snippet(path)) for path, name in SNIPPETS]
    walkthrough = ROOT / 'examples/document-call'
    source = next(source for _, name, source in sources if name == 'document_call')
    if not same_walkthrough((walkthrough / 'src/main.rs').read_bytes(), source):
        raise ValueError('Walkthrough fence differs from examples/document-call/src/main.rs')
    env = {**os.environ, 'CARGO_TARGET_DIR': str(Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve())}
    manifest = walkthrough / 'Cargo.toml'
    run(['cargo', 'fmt', '--manifest-path', manifest, '--all', '--check'])
    run(['cargo', 'test', '--offline', '--locked', '--manifest-path', manifest], env=env)
    run(['cargo', 'clippy', '--offline', '--locked', '--manifest-path', manifest,
         '--all-targets', '--', '-D', 'warnings'], env=env)
    # Reuse the workspace's locked Cargo graph for standalone prose compilation.
    build = run([
        'cargo', 'build', '--offline', '--locked', '--tests', '-p', 'openbindings-invocation',
        '-p', 'openbindings-json-schema-evaluator',
        '--message-format=json-render-diagnostics',
    ], stdout=subprocess.PIPE, text=True)
    libraries = {}
    for line in build.stdout.splitlines():
        message = json.loads(line)
        if message['reason'] == 'compiler-artifact' and message['target']['name'] in CRATES:
            for filename in message['filenames']:
                if filename.endswith('.rlib'):
                    libraries[message['target']['name']] = Path(filename)
        elif message['reason'] == 'compiler-message':
            print(message['message']['rendered'] or message['message']['message'], end='')
    if set(libraries) != set(CRATES):
        raise RuntimeError(f'Cargo did not report snippet dependencies: {libraries}')
    # Repository prose is not included from outside an archived crate. Test it
    # here with the same locked dependencies, including every Rust fence.
    for document in ['README.md', 'docs/rust-first-use.md', 'docs/document-to-typed-result.md', 'docs/invocation.md', 'docs/schema-evaluators.md']:
        args = [os.environ.get('RUSTDOC', 'rustdoc'), '--test', '--edition=2024', ROOT / document]
        for crate, library in libraries.items():
            args += ['--extern', f'{crate}={library}', '-L', f'dependency={library.parent}']
        run(args)
    with tempfile.TemporaryDirectory(prefix='sdk-rust-snippets-') as temporary:
        for path, name, source in sources:
            print(f'Checking verbatim Rust fence: {path}', flush=True)
            main_rs = Path(temporary) / f'{name}.rs'
            main_rs.write_bytes(source)
            executable = Path(temporary) / (name + ('.exe' if os.name == 'nt' else ''))
            args = [os.environ.get('RUSTC', 'rustc'), '--edition=2024', main_rs, '-o', executable]
            for crate, library in libraries.items():
                args += ['--extern', f'{crate}={library}', '-L', f'dependency={library.parent}']
            run(args)
            result = run([executable], stdout=subprocess.PIPE, text=True)
            print(result.stdout, end='')
            if name == 'document_call' and result.stdout != 'count: 7\n':
                raise AssertionError('Walkthrough must return the typed reply after completion')
            if name == 'evaluator_readme' and result.stdout != 'satisfies\n':
                raise AssertionError('Evaluator README example must demonstrate a satisfied input')


if __name__ == '__main__':
    run([sys.executable, ROOT / 'tools/test-rust-snippets.py'])
    main()
