#!/usr/bin/env python3
"""PROPOSED hosted preparation entrypoint. Builds npm archives; never runs SDK/browser jobs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tomllib
import urllib.request
from common import CANONICAL, HERE, bounded, check_preparer, check_seed, digest, file_binding, require, safe_extract, save, sha, tree, validate_request


class Preparation:
    def __init__(self, output):
        self.output = Path(output).resolve()
        self.output.mkdir()  # never overwrite or resume preparation
        self.commands = []
        self.env = {k: v for k, v in os.environ.items() if not any(x in k.upper() for x in ('TOKEN', 'SECRET', 'PASSWORD'))}
        for key in ('NODE_OPTIONS', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'):
            self.env.pop(key, None)
        self.env.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null', PYTHONDONTWRITEBYTECODE='1')

    def run(self, label, command, cwd=None, timeout=1800):
        number = len(self.commands)
        record = {'label': label, 'argv': [str(x) for x in command], 'cwd': str(cwd) if cwd else None, 'status': 'started'}
        self.commands.append(record); save(self.output / 'COMMANDS.json', self.commands)
        stdout, stderr = self.output / f'{number:03}-{label}.stdout', self.output / f'{number:03}-{label}.stderr'
        with stdout.open('wb') as out, stderr.open('wb') as err:
            try:
                completed = bounded(record['argv'], cwd=cwd, env=self.env, stdout=out, stderr=err, timeout=timeout,
                                    privileged_children=label in ('browser-install', 'create-canonical-directory'))
                record.update(status='completed', exitCode=completed.returncode)
            except subprocess.TimeoutExpired:
                record.update(status='timeout', exitCode=None)
                raise
            finally:
                record.update(stdoutSha256=sha(stdout), stderrSha256=sha(stderr))
                save(self.output / 'COMMANDS.json', self.commands)
        require(completed.returncode == 0, f'{label} failed; preserved logs; no automatic retry')
        require(stdout.stat().st_size <= 16 * 1024**2, 'command output too large to parse')
        return stdout.read_text()

    def download(self, url, destination, expected):
        digest(expected)
        require(url.startswith('https://'), 'HTTPS download required')
        receipt = {'url': url, 'expectedSha256': expected, 'status': 'started'}
        try:
            with urllib.request.urlopen(url, timeout=60) as response, Path(destination).open('xb') as out:
                receipt['responseUrl'] = response.url
                require(response.url.startswith('https://'), 'download redirected off HTTPS')
                count = 0
                while block := response.read(1024 * 1024):
                    count += len(block); require(count <= 512 * 1024**2, 'download size bound exceeded')
                    out.write(block)
            require(sha(destination) == expected, 'download digest mismatch; preserve failed bytes')
            receipt['status'] = 'verified'
        except Exception as error:
            receipt.update(status='failed', error=repr(error))
            raise
        finally:
            if Path(destination).is_file(): receipt.update(actualSha256=sha(destination), bytes=Path(destination).stat().st_size)
            save(self.output / (Path(destination).name + '.download.json'), receipt)

    def package(self, label, request, root, work):
        source = work / label
        git = ['git', '-c', 'core.hooksPath=/dev/null']
        start = len(self.commands)
        self.run(label + '-init', [*git, 'init', source], timeout=30)
        self.run(label + '-remote', [*git, '-C', source, 'remote', 'add', 'origin', 'https://github.com/' + request['sdkRepository'] + '.git'], timeout=30)
        self.run(label + '-fetch', [*git, '-C', source, 'fetch', '--depth=1', 'origin', request[label]['commit']], timeout=180)
        self.run(label + '-checkout', [*git, '-C', source, 'checkout', '--detach', 'FETCH_HEAD'], timeout=30)
        commit = self.run(label + '-head', [*git, '-C', source, 'rev-parse', 'HEAD'], timeout=30).strip()
        source_tree = self.run(label + '-tree', [*git, '-C', source, 'rev-parse', 'HEAD^{tree}'], timeout=30).strip()
        require(commit == request[label]['commit'] and source_tree == request[label]['tree'], 'SDK source binding differs')
        require(not self.run(label + '-clean-before', [*git, '-C', source, 'status', '--porcelain'], timeout=30).strip(), 'unclean source before build')
        require(tomllib.loads((source / 'rust-toolchain.toml').read_text())['toolchain']['channel'] == request['rustToolchain'], 'source Rust toolchain differs')
        package = source / 'packages/typescript'
        self.env['CARGO_TARGET_DIR'] = str(work / ('target-' + label))
        self.run(label + '-npm-ci', [root / 'node/bin/npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], package)
        self.run(label + '-wasm-build', [root / 'node/bin/npm', 'run', 'build:wasm'], package)
        self.run(label + '-typescript-build', [root / 'node/bin/npm', 'run', 'build'], package)
        archives = root / 'archives' / label; archives.mkdir(parents=True)
        packed = json.loads(self.run(label + '-npm-pack', [root / 'node/bin/npm', 'pack', '--ignore-scripts', '--json', '--pack-destination', archives], package))
        require(len(packed) == 1 and Path(packed[0]['filename']).name == packed[0]['filename'], 'ambiguous package archive')
        archive = archives / packed[0]['filename']
        expected = request[label]['expectedArchiveSha256']
        if expected is not None: require(sha(archive) == expected, 'archive differs from prospective expected binding')
        extraction = root / 'artifacts' / label
        safe_extract(archive, extraction, expected_top='package', max_bytes=128 * 1024**2)
        package_root = extraction / 'package'
        build = json.loads((package_root / 'dist/wasm/build-info.json').read_text())
        require(build['commit'] == commit and build['treeClean'] is True, 'package Wasm has stale/dirty source identity')
        require(build['rustSource']['sha256'] == request[label]['expectedRustSourceSha256'], 'compiler inputs differ from request')
        for name, expected_hash in build['artifacts'].items():
            require(Path(name).name == name and sha(package_root / 'dist/wasm' / name) == expected_hash, 'Wasm output identity mismatch')
        require(not self.run(label + '-clean-after', [*git, '-C', source, 'status', '--porcelain'], timeout=30).strip(), 'tracked source changed during packaging')
        require(self.run(label + '-head-after', [*git, '-C', source, 'rev-parse', 'HEAD'], timeout=30).strip() == commit, 'source commit changed')
        provenance = root / 'provenance'; provenance.mkdir(exist_ok=True)
        receipt_path = provenance / (label + '-source-package.json')
        receipt = {'sourceCommit': commit, 'sourceTree': source_tree, 'npmArchiveSha256': sha(archive),
                   'rustSourceSha256': build['rustSource']['sha256'], 'buildInfoSha256': sha(package_root / 'dist/wasm/build-info.json'),
                   'workspaceLockSha256': sha(source / 'Cargo.lock'), 'npmLockSha256': sha(package / 'package-lock.json'),
                   'commands': self.commands[start:], 'publicationPerformed': False, 'sdkRuntimeExecuted': False,
                   'referenceBaseline': request[label] if label == 'baseline' else None,
                   'scope': 'Fresh Linux npm archive from pinned source; archive/source identity, not behavioral qualification'}
        save(receipt_path, receipt)
        binding = {'sourceCommit': commit, 'sourceTree': source_tree, 'sourceReceipt': file_binding(receipt_path),
                   'archive': file_binding(archive), 'packageRoot': str(package_root), 'treeSha256': tree(package_root)[0],
                   'wasmSha256': sha(package_root / 'dist/wasm/openbindings_wasm_bg.wasm')}
        return binding


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--seed', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    preparation = Preparation(args.evidence)
    try:
        request = validate_request(json.loads(args.request.read_text()))
        preparer_freeze = check_preparer()
        check_seed(args.seed)
        require(sys.platform == 'linux' and platform.machine() == 'x86_64' and os.geteuid() != 0, 'unprivileged Ubuntu x86_64 runner required')
        release = Path('/etc/os-release').read_text()
        require('ID=ubuntu\n' in release and 'VERSION_ID="24.04"' in release, 'Ubuntu24.04 required')
        require(os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ.get('ImageVersion'), 'GitHub hosted image identity required')
        cgroup = Path('/sys/fs/cgroup')
        observed_host = {'osRelease': release, 'mountinfo': Path('/proc/self/mountinfo').read_text(),
                         'controllers': (cgroup / 'cgroup.controllers').read_text(),
                         'subtreeControl': (cgroup / 'cgroup.subtree_control').read_text(),
                         'procSwaps': Path('/proc/swaps').read_text(), 'uname': list(platform.uname())}
        save(preparation.output / 'HOST-READ-ONLY.json', observed_host)
        mounts = [line for line in observed_host['mountinfo'].splitlines() if line.split()[4] == str(cgroup)]
        require(len(mounts) == 1 and ' - cgroup2 ' in mounts[0], 'unified cgroup2 mount absent')
        require('memory' in observed_host['controllers'].split() and 'memory' in observed_host['subtreeControl'].split(),
                'root memory controller absent/disabled; no global controller changes permitted')
        migration = (cgroup / 'cgroup.procs').stat()
        require(migration.st_uid == 0 and migration.st_mode & 0o022 == 0, 'unsafe common-ancestor migration permissions')
        require(not CANONICAL.exists(), 'canonical destination already exists; no cleanup/reuse')
        root = CANONICAL
        preparation.run('create-canonical-directory', ['sudo', '-n', 'install', '-d', '-m', '755', '-o', str(os.getuid()), '-g', str(os.getgid()), root], timeout=30)
        work = preparation.output / 'work'; work.mkdir()
        downloads = root / 'downloads'; downloads.mkdir()
        node_archive = downloads / 'node-v22.19.0-linux-x64.tar.xz'
        preparation.download('https://nodejs.org/dist/v22.19.0/' + node_archive.name, node_archive, request['node']['archiveSha256'])
        safe_extract(node_archive, work / 'node-extraction', expected_top='node-v22.19.0-linux-x64')
        shutil.move(work / 'node-extraction/node-v22.19.0-linux-x64', root / 'node')
        preparation.env['PATH'] = str(root / 'node/bin') + ':' + preparation.env['PATH']
        require(preparation.run('node-version', [root / 'node/bin/node', '--version'], timeout=10).strip() == 'v22.19.0', 'Node version differs')
        require(preparation.run('npm-version', [root / 'node/bin/npm', '--version'], timeout=10).strip() == '10.9.3', 'npm version differs')
        preparation.run('git-version', ['git', '--version'], timeout=10)
        pw_archive = downloads / 'playwright-core-1.64.0.tgz'
        preparation.download('https://registry.npmjs.org/playwright-core/-/' + pw_archive.name, pw_archive, request['playwright']['archiveSha256'])
        safe_extract(pw_archive, work / 'playwright-extraction', expected_top='package', max_bytes=128 * 1024**2)
        shutil.move(work / 'playwright-extraction/package', root / 'playwright')
        require(json.loads((root / 'playwright/package.json').read_text())['version'] == '1.64.0', 'Playwright version differs')
        preparation.env['PLAYWRIGHT_BROWSERS_PATH'] = str(root / 'browsers')
        # Installs prerequisites/downloads; this does not launch Chromium or WebKit.
        preparation.run('browser-install', [root / 'node/bin/node', root / 'playwright/cli.js', 'install', '--with-deps', 'chromium', 'webkit'])
        preparation.env.update(CARGO_HOME=str(work / 'cargo'), RUSTUP_HOME=str(work / 'rustup'), CARGO_INCREMENTAL='0')
        rustup = Path(shutil.which('rustup')).resolve(strict=True)
        preparation.run('rustup-version', [rustup, '--version'], timeout=10)
        preparation.run('rust-toolchain-install', [rustup, 'toolchain', 'install', request['rustToolchain'], '--profile', 'minimal', '--target', 'wasm32-unknown-unknown'])
        cargo = Path(preparation.run('cargo-path', [rustup, 'which', '--toolchain', request['rustToolchain'], 'cargo'], timeout=30).strip())
        preparation.env['PATH'] = str(cargo.parent) + ':' + preparation.env['PATH']
        preparation.run('rustc-version', [cargo.parent / 'rustc', '-vV'], timeout=30)
        preparation.run('wasm-bindgen-install', [cargo, 'install', 'wasm-bindgen-cli', '--version', request['wasmBindgen'], '--locked', '--root', work / 'bindgen'])
        bindgen = work / 'bindgen/bin/wasm-bindgen'
        preparation.env['WASM_BINDGEN'] = str(bindgen)
        preparation.run('wasm-bindgen-version', [bindgen, '--version'], timeout=30)
        (root / 'artifacts').mkdir()
        artifacts = {label: preparation.package(label, request, root, work) for label in ('baseline', 'candidate')}
        shutil.copytree(args.seed / 'harness', root / 'harness')
        shutil.copytree(args.seed / 'fixtures', root / 'fixtures')
        check_seed(root)
        preparation.run('sdk-free-harness-tests', ['/usr/bin/python3', '-B', '-m', 'unittest', '-v', 'test_harness'], root / 'harness', timeout=60)
        # Query installed executable paths only. No launcher/SDK execution.
        paths = json.loads(preparation.run('browser-paths', [root / 'node/bin/node', '--input-type=module', '-e',
            "import {chromium,webkit} from '/opt/ob-memory-inputs/playwright/index.mjs'; console.log(JSON.stringify({chromium:chromium.executablePath(),webkit:webkit.executablePath()}));"], timeout=30))
        registry = json.loads((root / 'playwright/browsers.json').read_text())['browsers']
        browsers = {}
        for name in ('chromium', 'webkit'):
            launcher = Path(paths[name]).resolve(strict=True)
            require(launcher.is_relative_to(root / 'browsers'), 'browser launcher leaves pinned tree')
            browser_root = root / 'browsers' / launcher.relative_to(root / 'browsers').parts[0]
            actual = launcher if name == 'chromium' else browser_root / 'minibrowser-wpe/bin/MiniBrowser'
            require(actual.is_file() and actual.read_bytes()[:4] == b'\x7fELF', 'expected Linux browser executable layout absent; inspect preserved tree, do not guess')
            registered = next(row for row in registry if row['name'] == name)
            browsers[name] = {'root': str(browser_root), 'treeSha256': tree(browser_root)[0], 'registryName': name,
                              'revision': registered.get('revisionOverrides', {}).get('ubuntu24.04-x64', registered['revision']),
                              'version': registered['browserVersion'], 'launcher': file_binding(launcher), 'actualExecutable': file_binding(actual)}
        dpkg = preparation.run('dpkg-freeze', ['/usr/bin/dpkg-query', '-W', '-f=${binary:Package}\t${Version}\n'], timeout=30)
        python = Path('/usr/bin/python3').resolve(strict=True)
        bindings = {'schemaVersion': 1, 'host': {'imageVersion': os.environ['ImageVersion'], 'dpkgSha256': hashlib.sha256(dpkg.encode()).hexdigest()},
                    'artifacts': artifacts, 'node': {**file_binding(root / 'node/bin/node'), 'version': 'v22.19.0'},
                    'python': {**file_binding(python), 'version': preparation.run('python-version', [python, '--version'], timeout=10).strip()},
                    'playwright': {'root': str(root / 'playwright'), 'treeSha256': tree(root / 'playwright')[0], 'version': '1.64.0'},
                    'browsers': browsers, 'fixtures': {name: file_binding(root / 'fixtures' / (name + '.json')) for name in ('small', 'catalog', 'large', 'replacement')}}
        save(root / 'BINDINGS.proposed.json', bindings)
        shutil.copyfile(root / 'harness/OWNER-REVIEW.template.json', root / 'OWNER-REVIEW.pending.json')
        # Use the accepted harness's package archive equality check, without its
        # campaign gate, runtime initializer or any SDK import.
        sys.path.insert(0, str(root / 'harness'))
        import identity
        for binding in artifacts.values(): identity.package(binding)
        provenance = {'status': 'prepared-pending-binding-review', 'requestSha256': sha(args.request),
                      'preparerFreezeSha256': preparer_freeze,
                      'sourceFreezeSha256': sha(root / 'harness/SOURCE-FREEZE.json'), 'bindingsSha256': sha(root / 'BINDINGS.proposed.json'),
                      'runner': {k: os.environ.get(k) for k in ('ImageOS', 'ImageVersion', 'GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_SHA')},
                      'proposalHeadCommit': json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())['pull_request']['head']['sha'],
                      'installedTrees': {name: tree(root / name)[0] for name in ('node', 'playwright', 'browsers')},
                      'buildTools': {name: file_binding(path) for name, path in {'rustup': rustup, 'cargo': cargo, 'rustc': cargo.parent / 'rustc', 'wasm-bindgen': bindgen, 'git': Path(shutil.which('git')), 'node': root / 'node/bin/node', 'npm': root / 'node/bin/npm'}.items()},
                      'sdkRuntimeExecuted': False, 'browserLaunched': False, 'cgroupControlExecuted': False,
                      'baselineArchiveIsFreshLinuxRebuild': True, 'publicationPerformed': False}
        save(root / 'PREPARATION.json', provenance)
        preparer = root / 'preparer'; preparer.mkdir()
        source_freeze = json.loads((HERE / 'PREPARATION-SOURCE-FREEZE.json').read_text())
        for name in [*source_freeze['files'], 'PREPARATION-SOURCE-FREEZE.json']:
            # The seed already has its own directory; its manifest is preserved
            # here without making another copy of the registered fixture bytes.
            if name.startswith('seed/'): continue
            (preparer / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(HERE / name, preparer / name)
        shutil.copyfile(args.request, root / 'REQUEST.json')
        logs = root / 'preparation-logs'; logs.mkdir()
        for path in preparation.output.iterdir():
            if path.is_file(): shutil.copy2(path, logs / path.name)
        archive = preparation.output / 'prepared-inputs.tar.gz'
        with tarfile.open(archive, 'w:gz', dereference=False) as output:
            for path in sorted(root.iterdir()): output.add(path, arcname=path.name)
        save(preparation.output / 'RESULT.json', {**provenance, 'bundle': file_binding(archive), 'bundleBytes': archive.stat().st_size})
        return 0
    except Exception as error:
        save(preparation.output / 'FAILURE.json', {'status': 'preparation-failed', 'error': repr(error), 'sdkCampaignAttemptConsumed': False})
        return 1


if __name__ == '__main__': sys.exit(main())
