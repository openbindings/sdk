"""Portable source replay. Initial dependency acquisition may use the network."""
from pathlib import Path
import argparse, os, subprocess, sys
p = argparse.ArgumentParser()
p.add_argument('--browser', action='store_true')
p.add_argument('--tracked-files', type=Path, help='NUL-separated git ls-files list for a source archive')
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env.setdefault('CARGO_TARGET_DIR', str(root / 'target'))
def run(args, cwd=root):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True)
run([sys.executable, 'tools/verify-product-labels.py', '--self-test',
     *(['--tracked-files', a.tracked_files.resolve()] if a.tracked_files else [])])
run([sys.executable, 'tools/verify-doc-links.py'])
run([sys.executable, 'tools/verify-fixture-digests.py', '--self-test'])
run([sys.executable, 'tools/verify-advisory-policy.py', '--self-test'])
run([sys.executable, 'tools/test-reference.py'])
run([sys.executable, 'tools/verify-msrv.py', '--version', '1.88'])
run(['cargo', 'fmt', '--all', '--check'])
run(['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'openbindings-http-discovery/native', '--', '-D', 'warnings'])
run(['cargo', 'test', '--locked', '--workspace', '--features', 'openbindings-http-discovery/native'])
run(['cargo', 'test', '--locked', '-p', 'openbindings-internal-regress', '--lib'])
run([sys.executable, 'tools/verify-api-reference.py', '--rust-only'])
# Offline checks below read every platform's dependencies; fetch them all first.
run(['cargo', 'fetch', '--locked'])
run([sys.executable, 'tools/verify-dependency-isolation.py'])
run([sys.executable, 'tools/attribution.py', '--check'])
run([sys.executable, 'tools/verify-consumer-compat.py'])
for example in ['first_use', 'replacement', 'regex_budget']:
    run(['cargo', 'run', '--locked', '-p', 'openbindings-json-schema-evaluator', '--example', example])
run(['cargo', 'run', '--locked', '-p', 'openbindings-schema-evaluator-test-support', '--example', 'replay'])
run([sys.executable, 'tools/verify-rust-snippets.py'])
run([sys.executable, 'tools/verify-source-inspection.py'])
run([sys.executable, 'tools/verify-interface-synthesis.py'])
run([sys.executable, 'tools/verify-binding-invocation.py'])
run([sys.executable, 'tools/verify-invocation-resources.py'])
run([sys.executable, 'tools/verify-diagnostics.py'])
run([sys.executable, 'tools/test-source-inspection-verifier.py'])
run(['cargo', 'build', '--locked', '--release', '--workspace'])
run(['cargo', 'build', '--locked', '--manifest-path', 'tools/conformance-observer/Cargo.toml'])
run([sys.executable, 'tools/qualification/replay/corpus.py'])
run([sys.executable, 'tools/qualification/replay/production-corpus.py', '--binary', Path(env['CARGO_TARGET_DIR']) / 'debug' / ('sdk-production-observer.exe' if os.name == 'nt' else 'sdk-production-observer')])
if a.browser:
    package = root / 'packages/typescript'
    npm = 'npm.cmd' if os.name == 'nt' else 'npm'
    run([npm, 'ci', '--ignore-scripts'], package)
    for command in ['build:wasm', 'build', 'test', 'test:types', 'format:check', 'test:package']:
        run([npm, 'run', command], package)
    run([sys.executable, 'tools/verify-api-reference.py', '--typescript-only'])
    run([sys.executable, 'tools/qualification/replay/check-container-findings.py', '--binary', Path(env['CARGO_TARGET_DIR']) / 'debug' / ('sdk-production-observer.exe' if os.name == 'nt' else 'sdk-production-observer')])
    run([sys.executable, 'tools/verify-source-inspection.py', '--archives', '--wasm'])
    run([sys.executable, 'tools/verify-interface-synthesis.py', '--archives', '--wasm'])
    run([sys.executable, 'tools/verify-binding-invocation.py', '--archives', '--wasm'])
    run([sys.executable, 'tools/qualification/replay/prepare-browser-fixtures.py'])
    for engine in ['chromium', 'webkit']:
        run(['node', 'packages/typescript/test/browser.mjs', engine])
        run([sys.executable, 'tools/qualification/replay/judge-browser.py', 'tools/qualification/hosts/' + engine])
