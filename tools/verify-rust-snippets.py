"""Compile and run the two first-use Rust fences without rewriting their contents."""
from pathlib import Path
import json
import os
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SNIPPETS = [
    (ROOT / 'docs/rust-first-use.md', 'rust_first_use'),
    (ROOT / 'crates/openbindings-json-schema-evaluator/README.md', 'evaluator_readme'),
]
CRATES = ['openbindings', 'openbindings_json_schema_evaluator']


def read_snippet(path):
    markdown = path.read_bytes()
    openings = re.findall(rb'^```rust[^\r\n]*\r?$', markdown, re.M)
    snippets = re.findall(rb'^```rust\r?\n(.*?)^```[ \t]*\r?$', markdown, re.M | re.S)
    if len(openings) != 1 or len(snippets) != 1:
        raise ValueError(f'{path}: expected exactly one complete Rust fence, '
                         f'found {len(openings)} openings and {len(snippets)} complete fences')
    return snippets[0]


def run(args, **kwargs):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), cwd=ROOT, check=True, **kwargs)


def main():
    # Require both documented examples before building; a deleted fence is a failure.
    sources = [(path, name, read_snippet(path)) for path, name in SNIPPETS]
    # Reuse the locked Cargo graph rather than resolve a separate consumer manifest.
    build = run([
        'cargo', 'build', '--locked', '-p', 'openbindings-json-schema-evaluator',
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
        raise RuntimeError(f'Cargo did not report both snippet dependencies: {libraries}')
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
            run([executable])


if __name__ == '__main__':
    main()
