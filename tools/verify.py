"""Portable source replay. Initial dependency acquisition may use the network."""
from pathlib import Path
import argparse, os, subprocess, sys
p = argparse.ArgumentParser()
p.add_argument('--browser', action='store_true')
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env.setdefault('CARGO_TARGET_DIR', str(root / 'target'))
def run(args, cwd=root):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True)
run(['cargo', 'fmt', '--all', '--check'])
run(['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'openbindings-http-discovery/native', '--', '-D', 'warnings'])
run(['cargo', 'test', '--locked', '--workspace', '--features', 'openbindings-http-discovery/native'])
run(['cargo', 'build', '--locked', '--release', '--workspace'])
run(['cargo', 'build', '--locked', '--manifest-path', 'tools/conformance-observer/Cargo.toml'])
run([sys.executable, 'tools/qualification/replay/corpus.py'])
run([sys.executable, 'tools/qualification/replay/production-corpus.py', '--binary', Path(env['CARGO_TARGET_DIR']) / 'debug' / ('sdk-production-observer.exe' if os.name == 'nt' else 'sdk-production-observer')])
if a.browser:
    package = root / 'packages/typescript'
    npm = 'npm.cmd' if os.name == 'nt' else 'npm'
    run([npm, 'ci', '--ignore-scripts'], package)
    for command in ['build:wasm', 'build', 'test', 'test:types', 'format:check']:
        run([npm, 'run', command], package)
    run([sys.executable, 'tools/qualification/replay/prepare-browser-fixtures.py'])
    for engine in ['chromium', 'webkit']:
        run(['node', 'packages/typescript/test/browser.mjs', engine])
        run([sys.executable, 'tools/qualification/replay/judge-browser.py', 'tools/qualification/hosts/' + engine])
