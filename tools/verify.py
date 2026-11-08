"""Portable source replay. Initial dependency acquisition may use the network."""
from pathlib import Path
import argparse, json, os, subprocess, sys
p = argparse.ArgumentParser()
p.add_argument('--browser', action='store_true')
p.add_argument('--tracked-files', type=Path, help='NUL-separated git ls-files list for a source archive')
a = p.parse_args()
root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env.setdefault('CARGO_TARGET_DIR', str(root / 'target'))
def run(args, cwd=root, stdout=None):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, stdout=stdout, check=True)
run([sys.executable, 'tools/verify-product-labels.py', '--self-test',
     *(['--tracked-files', a.tracked_files.resolve()] if a.tracked_files else [])])
run([sys.executable, 'tools/verify-doc-links.py'])
run([sys.executable, 'tools/verify-fixture-digests.py', '--self-test'])
run([sys.executable, 'tools/verify-evaluator-inputs.py', '--self-test'])
run([sys.executable, 'tools/verify-authority-pins.py', '--self-test'])
run([sys.executable, 'tools/verify-advisory-policy.py', '--self-test'])
run([sys.executable, 'tools/verify-publication-guards.py', '--self-test'])
run([sys.executable, 'tools/package-candidate.py', '--self-test'])
run([sys.executable, 'tools/test-reference.py'])
run([sys.executable, 'tools/verify-msrv.py', '--version', '1.88'])
run(['cargo', 'fmt', '--all', '--check'])
run(['cargo', 'fmt', '--manifest-path', 'fuzz/Cargo.toml', '--all', '--check'])
run(['cargo', 'fmt', '--manifest-path', 'fuzz/seeds/Cargo.toml', '--all', '--check'])
run(['cargo', 'run', '--locked', '--manifest-path', 'tools/identifier-ranges/Cargo.toml', '--', '--check'])
run(['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'openbindings-http-discovery/native', '--', '-D', 'warnings'])
run(['cargo', 'test', '--locked', '--workspace', '--no-fail-fast', '--features', 'openbindings-http-discovery/native'])
run(['cargo', 'test', '--locked', '--manifest-path', 'fuzz/seeds/Cargo.toml', '--test', 'seed_corpus'])
run([sys.executable, 'tools/verify-api-reference.py', '--rust-only'])
# Offline checks below read every platform's dependencies; fetch them all first.
run(['cargo', 'fetch', '--locked'])
run([sys.executable, 'tools/verify-dependency-isolation.py'])
run([sys.executable, 'tools/attribution.py', '--check'])
run([sys.executable, 'tools/verify-consumer-compat.py'])
for example in ['first_use', 'replacement']:
    run(['cargo', 'run', '--locked', '-p', 'openbindings-json-schema-evaluator', '--example', example])
# Preserve the full multi-megabyte receipt without feeding one enormous line to
# the host's console/CI log transport. The example's success assertion still runs.
replay_receipt = Path(env['CARGO_TARGET_DIR']) / 'qualification' / 'evaluator-replay.json'
if not replay_receipt.is_absolute():
    replay_receipt = root / replay_receipt
replay_receipt.parent.mkdir(parents=True, exist_ok=True)
with replay_receipt.open('wb') as output:
    run(['cargo', 'run', '--locked', '-p', 'openbindings-json-schema-evaluator', '--example', 'replay'], stdout=output)
with replay_receipt.open(encoding='utf-8') as source:
    replay = json.load(source)
print(f"Evaluator replay: {len(replay['report']['observations'])} observations; full receipt: {replay_receipt}", flush=True)
run([sys.executable, 'tools/verify-rust-snippets.py'])
run([sys.executable, 'tools/verify-source-pull.py'])
run([sys.executable, 'tools/verify-invocation.py'])
run([sys.executable, 'tools/verify-kind-adapter.py'])
run([sys.executable, 'tools/verify-invocation-resources.py'])
run([sys.executable, 'tools/verify-diagnostics.py'])
run([sys.executable, 'tools/literal-scratch/run.py'])
run([sys.executable, 'tools/decimal-scratch/run.py', '--release'])
run([sys.executable, 'tools/test-interface-verifiers.py'])
run(['cargo', 'build', '--locked', '--release', '--workspace'])
# Release builds carry the pattern stack qualification on every host, Windows included.
run(['cargo', 'test', '--locked', '--release', '-p', 'openbindings-json-schema-evaluator', '--test', 'pattern_stack'])
run(['cargo', 'test', '--locked', '--release', '-p', 'openbindings-json-schema-evaluator', '--test', 'evaluation_stack'])
run(['cargo', 'test', '--locked', '--release', '-p', 'openbindings-json-schema-evaluator', '--test', 'walk_stack'])
# Compiled recursion witnesses and the large diamond values are lib tests run
# in release on every host.
run(['cargo', 'test', '--locked', '--release', '-p', 'openbindings-json-schema-evaluator', '--lib', '--', '--exact',
     'compiled_qualification::recursive_compiled_boundary_runs_on_qualified_native_threads',
     'compiled_qualification::ordinary_recursive_boundaries_run_on_qualified_native_threads',
     'compiled_qualification::productive_scalar_recursive_boundaries_fit_qualified_threads',
     'compiled_qualification::wasm_depth_walk_bounds_match_native_selection',
     'compiled_qualification::expansion::product_default_large_diamonds_finish_without_backend_entry'])
# Original-node frames have separate native stack witnesses; they are not covered
# by the outgoing compiler's three integration-test targets above.
run(['cargo', 'test', '--locked', '--release', '-p', 'openbindings-json-schema-evaluator', '--lib',
     'semantic::execution::tests::original_frame', '--', '--ignored', '--nocapture'])
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
    run([sys.executable, 'tools/verify-source-pull.py', '--archives', '--wasm'])
    run([sys.executable, 'tools/verify-invocation.py', '--archives', '--wasm'])
    run([sys.executable, 'tools/verify-kind-adapter.py', '--wasm'])
    run([sys.executable, 'tools/qualification/replay/prepare-browser-fixtures.py'])
    for engine in ['chromium', 'webkit']:
        run(['node', 'packages/typescript/test/browser.mjs', engine])
        run([sys.executable, 'tools/qualification/replay/judge-browser.py', 'tools/qualification/hosts/' + engine])
